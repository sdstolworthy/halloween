//! Browser emulator: the real `eyes-core` state machine and `eyes-render` renderer behind a raw wasm ABI
//! (no wasm-bindgen, no JS build step), mirroring flight-web.
//!
//! Contract with `web/main.js`:
//! - The framebuffer is 128x64 RGBA8 at `framebuffer_ptr()`; it is valid after `frame(now_ms)`.
//! - Config params are addressed by index into [`Param::ALL`]; their keys and ranges are exported so the
//!   page builds its controls from the same table the firmware validates against.
//! - Text crosses the boundary through two fixed buffers: JS writes UTF-8 into `input_ptr()` and calls
//!   `apply_config_json(len)`; the last rejection reason is readable at `message_ptr()`/`message_len()`.
//! - Times are milliseconds from `requestAnimationFrame`, monotonic within a page load.
//! - Distances go in the way the device receives them: [`porch_sensor_payload`] formats them exactly as
//!   porch-sensor publishes, and the text goes through the device's own `parse_distance`.
//!
//! The panel's display stage (gamma) is a separate, optional step on the RGBA copy; see [`panel`].

pub mod panel;

use eyes_core::geometry::{PanelFrame, Vec3};
use eyes_core::layout::{PANEL_HEIGHT, PANEL_WIDTH};
use eyes_core::payload::SENSOR_MAX_RANGE_M;
use eyes_core::{parse_distance, Eyes, EyesConfig, Mode, Param, Pose, SplitMix64};
use eyes_render::Framebuffer;

const FRAMEBUFFER_LEN: usize = (PANEL_WIDTH * PANEL_HEIGHT * 4) as usize;
const INPUT_CAPACITY: usize = 1024;
const MESSAGE_CAPACITY: usize = 256;

pub struct Emulator {
    eyes: Eyes<SplitMix64>,
    framebuffer: Framebuffer,
    rgba: [u8; FRAMEBUFFER_LEN],
    input: [u8; INPUT_CAPACITY],
    message: [u8; MESSAGE_CAPACITY],
    message_len: usize,
    sensor_payload: String,
    last_pose: Pose,
}

impl Emulator {
    pub fn new(seed: u32, now_ms: u64) -> Self {
        Self {
            eyes: Eyes::new(
                EyesConfig::DEFAULT,
                SplitMix64::new(u64::from(seed)),
                now_ms,
            ),
            framebuffer: Framebuffer::new(),
            rgba: [0; FRAMEBUFFER_LEN],
            input: [0; INPUT_CAPACITY],
            message: [0; MESSAGE_CAPACITY],
            message_len: 0,
            sensor_payload: String::new(),
            last_pose: Pose::RESTING,
        }
    }

    /// Restart the state machine with a new seed, keeping the current config.
    pub fn reset(&mut self, seed: u32, now_ms: u64) {
        let config = *self.eyes.config();
        self.eyes = Eyes::new(config, SplitMix64::new(u64::from(seed)), now_ms);
    }

    pub fn config(&self) -> &EyesConfig {
        self.eyes.config()
    }

    pub fn set_param(&mut self, param: Param, value: f32) -> bool {
        self.apply(self.eyes.config().with(param, value))
    }

    /// Merge a JSON patch over the defaults, exactly as the device treats the retained config topic.
    pub fn apply_config_json(&mut self, json: &[u8]) -> bool {
        self.apply(EyesConfig::DEFAULT.merged_with_json(json))
    }

    fn apply(&mut self, result: Result<EyesConfig, eyes_core::ConfigError>) -> bool {
        match result {
            Ok(config) => {
                self.eyes.set_config(config);
                self.set_message("");
                true
            }
            Err(error) => {
                self.set_message(&error.to_string());
                false
            }
        }
    }

    fn set_message(&mut self, text: &str) {
        // Truncate on a char boundary so JS always decodes valid UTF-8.
        let mut len = text.len().min(MESSAGE_CAPACITY);
        while !text.is_char_boundary(len) {
            len -= 1;
        }
        self.message[..len].copy_from_slice(&text.as_bytes()[..len]);
        self.message_len = len;
    }

    /// Where JS writes a UTF-8 JSON patch before calling [`Emulator::apply_input_json`].
    pub fn input_buffer(&mut self) -> &mut [u8; INPUT_CAPACITY] {
        &mut self.input
    }

    pub fn apply_input_json(&mut self, len: usize) -> bool {
        let json = self.input[..len.min(INPUT_CAPACITY)].to_vec();
        self.apply_config_json(&json)
    }

    pub fn message(&self) -> &str {
        core::str::from_utf8(&self.message[..self.message_len]).unwrap_or_default()
    }

    /// Handle a distance-topic payload exactly as the device does. Returns false (with the reason in
    /// [`Emulator::message`]) for a payload the device would ignore.
    pub fn receive_distance(&mut self, payload: &[u8], now_ms: u64) -> bool {
        match parse_distance(payload) {
            Ok(reading) => {
                self.eyes.observe(reading, now_ms);
                true
            }
            Err(error) => {
                self.set_message(&error.to_string());
                false
            }
        }
    }

    /// Play porch-sensor seeing something `distance_m` away (`None` = no echo).
    pub fn sensor_reading(&mut self, distance_m: Option<f32>, now_ms: u64) {
        let payload = porch_sensor_payload(distance_m);
        self.receive_distance(payload.as_bytes(), now_ms);
        self.sensor_payload = payload;
    }

    /// The last payload [`Emulator::sensor_reading`] sent.
    pub fn sensor_payload(&self) -> &str {
        &self.sensor_payload
    }

    /// Advance to `now_ms` and render the resulting pose into the RGBA framebuffer.
    pub fn frame(&mut self, now_ms: u64) -> Pose {
        let pose = self.eyes.tick(now_ms);
        let Ok(()) = eyes_render::draw(&pose, &mut self.framebuffer);
        self.framebuffer.write_rgba(&mut self.rgba);
        self.last_pose = pose;
        pose
    }

    /// Lid closure of the last frame, 0 = open, 1 = shut (blink and squint combined, as drawn).
    pub fn lid(&self) -> f32 {
        self.last_pose.blink.max(self.last_pose.squint)
    }

    pub fn gaze_moving(&self) -> bool {
        self.eyes.gaze_moving()
    }

    pub fn mode(&self) -> Mode {
        self.eyes.mode()
    }

    pub fn rgba(&self) -> &[u8; FRAMEBUFFER_LEN] {
        &self.rgba
    }
}

/// What porch-sensor publishes on the distance topic: metres to two decimals, or `none` for no echo and
/// anything at or beyond its range (mirrors the `mqtt.publish` lambda in porch-sensor.yaml).
pub fn porch_sensor_payload(distance_m: Option<f32>) -> String {
    match distance_m {
        Some(metres) if metres.is_finite() && metres < SENSOR_MAX_RANGE_M => {
            format!("{:.2}", metres.max(0.0))
        }
        _ => "none".into(),
    }
}

/// Physical eye positions for the geometry diagrams, screen-left eye first.
pub fn eye_positions(config: &EyesConfig) -> [Vec3; 2] {
    PanelFrame::new(config).eye_positions(config.pixel_pitch_mm)
}

/// A `requestAnimationFrame` timestamp as whole milliseconds; anything unusable becomes 0.
pub fn millis(now_ms: f64) -> u64 {
    if now_ms.is_finite() && now_ms > 0.0 {
        now_ms as u64
    } else {
        0
    }
}

#[cfg(target_arch = "wasm32")]
mod abi {
    //! The `extern "C"` surface. Thin by design: every export forwards to [`Emulator`] or the core.

    use super::*;
    use eyes_core::geometry::gaze_angles;

    static mut EMULATOR: Option<Emulator> = None;

    /// The browser calls exports one at a time on a single thread and none of them re-enters another, so
    /// handing out one `&mut` at a time is sound.
    fn emulator() -> &'static mut Emulator {
        // SAFETY: see above; this helper is the only place a reference to the static is formed.
        unsafe { (*core::ptr::addr_of_mut!(EMULATOR)).get_or_insert_with(|| Emulator::new(1, 0)) }
    }

    fn axis(vector: Vec3, index: u32) -> f32 {
        match index {
            0 => vector.x,
            1 => vector.y,
            _ => vector.z,
        }
    }

    fn param(index: u32) -> Option<Param> {
        Param::ALL.get(index as usize).copied()
    }

    #[no_mangle]
    pub extern "C" fn width() -> u32 {
        PANEL_WIDTH
    }

    #[no_mangle]
    pub extern "C" fn height() -> u32 {
        PANEL_HEIGHT
    }

    #[no_mangle]
    pub extern "C" fn framebuffer_ptr() -> *const u8 {
        emulator().rgba().as_ptr()
    }

    #[no_mangle]
    pub extern "C" fn framebuffer_len() -> usize {
        FRAMEBUFFER_LEN
    }

    #[no_mangle]
    pub extern "C" fn reset(seed: u32, now_ms: f64) {
        emulator().reset(seed, millis(now_ms));
    }

    #[no_mangle]
    pub extern "C" fn param_count() -> u32 {
        Param::ALL.len() as u32
    }

    #[no_mangle]
    pub extern "C" fn param_key_ptr(index: u32) -> *const u8 {
        param(index).map_or(core::ptr::null(), |param| param.spec().key.as_ptr())
    }

    #[no_mangle]
    pub extern "C" fn param_key_len(index: u32) -> usize {
        param(index).map_or(0, |param| param.spec().key.len())
    }

    #[no_mangle]
    pub extern "C" fn param_min(index: u32) -> f32 {
        param(index).map_or(f32::NAN, |param| param.spec().min)
    }

    #[no_mangle]
    pub extern "C" fn param_max(index: u32) -> f32 {
        param(index).map_or(f32::NAN, |param| param.spec().max)
    }

    #[no_mangle]
    pub extern "C" fn param_step(index: u32) -> f32 {
        param(index).map_or(f32::NAN, |param| param.spec().step)
    }

    #[no_mangle]
    pub extern "C" fn param_default(index: u32) -> f32 {
        param(index).map_or(f32::NAN, |param| EyesConfig::DEFAULT.get(param))
    }

    #[no_mangle]
    pub extern "C" fn param_value(index: u32) -> f32 {
        param(index).map_or(f32::NAN, |param| emulator().config().get(param))
    }

    /// Returns 1 if accepted, 0 if rejected (reason at `message_ptr`).
    #[no_mangle]
    pub extern "C" fn set_param(index: u32, value: f32) -> u32 {
        param(index).is_some_and(|param| emulator().set_param(param, value)) as u32
    }

    #[no_mangle]
    pub extern "C" fn input_ptr() -> *mut u8 {
        emulator().input_buffer().as_mut_ptr()
    }

    #[no_mangle]
    pub extern "C" fn input_capacity() -> usize {
        INPUT_CAPACITY
    }

    /// Apply the JSON patch JS wrote into the first `len` bytes of the input buffer. 1 = applied.
    #[no_mangle]
    pub extern "C" fn apply_config_json(len: usize) -> u32 {
        emulator().apply_input_json(len) as u32
    }

    #[no_mangle]
    pub extern "C" fn message_ptr() -> *const u8 {
        emulator().message().as_ptr()
    }

    #[no_mangle]
    pub extern "C" fn message_len() -> usize {
        emulator().message().len()
    }

    /// porch-sensor sees something `metres` away; NaN means no echo.
    #[no_mangle]
    pub extern "C" fn sensor_reading(metres: f32, now_ms: f64) {
        let distance = (!metres.is_nan()).then_some(metres);
        emulator().sensor_reading(distance, millis(now_ms));
    }

    #[no_mangle]
    pub extern "C" fn sensor_payload_ptr() -> *const u8 {
        emulator().sensor_payload().as_ptr()
    }

    #[no_mangle]
    pub extern "C" fn sensor_payload_len() -> usize {
        emulator().sensor_payload().len()
    }

    /// 256-entry lookup from rendered channel value to what the panel shows, for the page's optional
    /// "panel gamma" display stage.
    #[no_mangle]
    pub extern "C" fn panel_display_lut_ptr() -> *const u8 {
        static mut LUT: Option<[u8; 256]> = None;
        // SAFETY: single-threaded, non-reentrant ABI (see `emulator`).
        unsafe {
            (*core::ptr::addr_of_mut!(LUT))
                .get_or_insert_with(panel::display_lut)
                .as_ptr()
        }
    }

    #[no_mangle]
    pub extern "C" fn frame(now_ms: f64) {
        emulator().frame(millis(now_ms));
    }

    #[no_mangle]
    pub extern "C" fn lid() -> f32 {
        emulator().lid()
    }

    #[no_mangle]
    pub extern "C" fn gaze_moving() -> u32 {
        emulator().gaze_moving() as u32
    }

    /// 0 = idle, 1 = tracking.
    #[no_mangle]
    pub extern "C" fn mode() -> u32 {
        (emulator().mode() == Mode::Tracking) as u32
    }

    #[no_mangle]
    pub extern "C" fn gaze_yaw_deg(eye: u32, distance_m: f32) -> f32 {
        gaze_angles(emulator().config(), distance_m)[eye.min(1) as usize].yaw_deg
    }

    #[no_mangle]
    pub extern "C" fn gaze_pitch_deg(eye: u32, distance_m: f32) -> f32 {
        gaze_angles(emulator().config(), distance_m)[eye.min(1) as usize].pitch_deg
    }

    /// World coordinate `axis` (0 = x, 1 = y, 2 = z) of an eye centre.
    #[no_mangle]
    pub extern "C" fn eye_position(eye: u32, axis_index: u32) -> f32 {
        axis(
            eye_positions(emulator().config())[eye.min(1) as usize],
            axis_index,
        )
    }

    /// World coordinate `axis` of the panel's outward normal.
    #[no_mangle]
    pub extern "C" fn panel_normal(axis_index: u32) -> f32 {
        axis(PanelFrame::new(emulator().config()).normal, axis_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scripted run `web/smoke.mjs` replays through the wasm build: seed 1, a visitor holding at
    /// 1.2 m reported every 100 ms, frames every 33 ms for 3 s.
    fn scripted_frame() -> Emulator {
        let mut emulator = Emulator::new(1, 0);
        let mut now_ms = 0;
        while now_ms < 3000 {
            if now_ms % 100 < 33 {
                emulator.sensor_reading(Some(1.2), now_ms);
            }
            now_ms += 33;
            emulator.frame(now_ms);
        }
        emulator
    }

    fn fnv1a(bytes: &[u8]) -> u32 {
        bytes.iter().fold(0x811C_9DC5, |hash, &byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        })
    }

    /// Keep in sync with `SCRIPTED_HASH` in web/smoke.mjs; equal hashes prove the browser renders exactly
    /// what the host (and so the firmware) renders.
    #[test]
    fn scripted_run_matches_the_smoke_test_hash() {
        assert_eq!(fnv1a(scripted_frame().rgba()), 0xF99B_39D7);
    }

    #[test]
    fn scripted_run_is_tracking() {
        assert_eq!(scripted_frame().mode(), Mode::Tracking);
    }

    #[test]
    fn lid_reports_the_last_frames_closure() {
        let mut emulator = Emulator::new(1, 0);
        let pose = emulator.frame(100);
        assert_eq!(emulator.lid(), pose.blink.max(pose.squint));
    }

    #[test]
    fn sensor_payloads_match_porch_sensor() {
        let cases = [
            (Some(1.834), "1.83"),
            (Some(0.0), "0.00"),
            (Some(4.49), "4.49"),
            (Some(4.5), "none"),
            (Some(7.0), "none"),
            (Some(f32::INFINITY), "none"),
            (None, "none"),
        ];
        for (distance, want) in cases {
            assert_eq!(porch_sensor_payload(distance), want, "{distance:?}");
        }
    }

    #[test]
    fn beyond_range_readings_never_track() {
        let mut emulator = Emulator::new(1, 0);
        for now_ms in (0..2000).step_by(100) {
            emulator.sensor_reading(Some(4.6), now_ms);
            emulator.frame(now_ms);
        }
        assert_eq!(emulator.mode(), Mode::Idle);
    }

    #[test]
    fn malformed_distance_payloads_are_rejected_with_a_reason() {
        let mut emulator = Emulator::new(1, 0);
        let accepted = emulator.receive_distance(b"1.2m", 0);
        assert!(!accepted && !emulator.message().is_empty());
    }

    #[test]
    fn set_param_rejects_out_of_range_values_with_a_reason() {
        let mut emulator = Emulator::new(1, 0);
        let accepted = emulator.set_param(Param::MaxGaze, 120.0);
        assert!(
            !accepted && emulator.message().contains("max_gaze_deg"),
            "{}",
            emulator.message()
        );
    }

    #[test]
    fn set_param_updates_the_config() {
        let mut emulator = Emulator::new(1, 0);
        emulator.set_param(Param::EyesHeight, 2.2);
        assert_eq!(emulator.config().eyes_height_m, 2.2);
    }

    #[test]
    fn config_json_merges_over_defaults_not_the_current_config() {
        let mut emulator = Emulator::new(1, 0);
        emulator.set_param(Param::EyesHeight, 2.2);
        emulator.apply_config_json(br#"{"face_height_m": 1.2}"#);
        let expected = EyesConfig {
            face_height_m: 1.2,
            ..EyesConfig::DEFAULT
        };
        assert_eq!(*emulator.config(), expected);
    }

    #[test]
    fn rejected_json_keeps_the_config_and_explains() {
        let mut emulator = Emulator::new(1, 0);
        let accepted = emulator.apply_config_json(br#"{"bogus": 1}"#);
        let unchanged = *emulator.config() == EyesConfig::DEFAULT;
        assert!(!accepted && unchanged && emulator.message().contains("bogus"));
    }

    #[test]
    fn input_buffer_json_is_applied_up_to_len() {
        let mut emulator = Emulator::new(1, 0);
        let json = br#"{"eyes_height_m": 2.0}garbage"#;
        emulator.input_buffer()[..json.len()].copy_from_slice(json);
        assert!(emulator.apply_input_json(json.len() - "garbage".len()));
    }

    #[test]
    fn reset_keeps_the_config() {
        let mut emulator = Emulator::new(1, 0);
        emulator.set_param(Param::FaceHeight, 1.1);
        emulator.reset(9, 500);
        assert_eq!(emulator.config().face_height_m, 1.1);
    }

    #[test]
    fn long_messages_are_truncated_on_a_char_boundary() {
        let mut emulator = Emulator::new(1, 0);
        let long = "é".repeat(MESSAGE_CAPACITY);
        emulator.set_message(&long);
        assert_eq!(emulator.message().len(), MESSAGE_CAPACITY);
    }

    #[test]
    fn millis_clamps_unusable_times_to_zero() {
        let cases = [(f64::NAN, 0), (-5.0, 0), (f64::INFINITY, 0), (1234.9, 1234)];
        for (input, want) in cases {
            assert_eq!(millis(input), want, "{input}");
        }
    }

    #[test]
    fn eye_positions_straddle_the_panel_centre() {
        let [left, right] = eye_positions(&EyesConfig::DEFAULT);
        let centre_x = (left.x + right.x) / 2.0;
        assert!((centre_x - EyesConfig::DEFAULT.eyes_offset_x_m).abs() < 1e-6);
    }
}

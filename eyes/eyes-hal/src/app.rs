use eyes_core::topics::{self, Topic};
use eyes_core::{parse_distance, Eyes, EyesConfig, Mode, Rng};
use log::{info, warn};

use crate::{Clock, Inbox, Matrix, Message};

/// A flood of messages must never stall the animation; anything beyond this waits for the next frame.
pub const MAX_MESSAGES_PER_FRAME: usize = 16;

/// The device loop: drain messages, advance the eyes, draw, present. Call [`App::frame`] at the frame
/// rate; it never blocks and never panics on bad input, so the eyes keep animating whatever the network
/// does.
pub struct App<M: Matrix, C: Clock, I: Inbox, R: Rng> {
    matrix: M,
    clock: C,
    inbox: I,
    eyes: Eyes<R>,
    /// The provisioned config; each retained config message is merged over this, not over the previous
    /// message, so the retained payload alone describes the device's state.
    defaults: EyesConfig,
    brightness: u8,
}

impl<M: Matrix, C: Clock, I: Inbox, R: Rng> App<M, C, I, R> {
    pub fn new(mut matrix: M, clock: C, inbox: I, defaults: EyesConfig, rng: R) -> Self {
        matrix.set_brightness(defaults.brightness);
        let eyes = Eyes::new(defaults, rng, clock.uptime_ms());
        Self {
            matrix,
            clock,
            inbox,
            eyes,
            defaults,
            brightness: defaults.brightness,
        }
    }

    pub fn config(&self) -> &EyesConfig {
        self.eyes.config()
    }

    pub fn mode(&self) -> Mode {
        self.eyes.mode()
    }

    pub fn frame(&mut self) {
        let now_ms = self.clock.uptime_ms();
        for _ in 0..MAX_MESSAGES_PER_FRAME {
            let Some(message) = self.inbox.poll() else {
                break;
            };
            self.handle(&message, now_ms);
        }
        let pose = self.eyes.tick(now_ms);
        if eyes_render::draw(&pose, &mut self.matrix).is_err() {
            warn!("drawing the eyes failed");
        }
        if self.matrix.flush().is_err() {
            warn!("presenting the frame failed");
        }
    }

    fn handle(&mut self, message: &Message, now_ms: u64) {
        match topics::route(&message.topic) {
            Some(Topic::Distance) => match parse_distance(&message.payload) {
                Ok(reading) => self.eyes.observe(reading, now_ms),
                Err(error) => warn!(
                    "ignoring distance {:?}: {error}",
                    printable(&message.payload)
                ),
            },
            Some(Topic::Config) => self.apply_config(&message.payload),
            None => {}
        }
    }

    fn apply_config(&mut self, payload: &[u8]) {
        match self.defaults.merged_with_json(payload) {
            Ok(config) => {
                info!("config applied: {config:?}");
                self.eyes.set_config(config);
                if config.brightness != self.brightness {
                    self.brightness = config.brightness;
                    self.matrix.set_brightness(config.brightness);
                }
            }
            Err(error) => warn!("config rejected, keeping the current one: {error}"),
        }
    }
}

/// Enough of a payload to recognise in a log line without dumping a large or binary message.
fn printable(payload: &[u8]) -> &str {
    let head = &payload[..payload.len().min(32)];
    core::str::from_utf8(head).unwrap_or("<not UTF-8>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{FakeClock, MockMatrix, QueueInbox};
    use eyes_core::SplitMix64;

    struct Rig {
        app: App<MockMatrix, FakeClock, QueueInbox, SplitMix64>,
        matrix: MockMatrix,
        clock: FakeClock,
        inbox: QueueInbox,
    }

    fn rig() -> Rig {
        let (matrix, clock, inbox) = (
            MockMatrix::new(),
            FakeClock::default(),
            QueueInbox::default(),
        );
        let app = App::new(
            matrix.clone(),
            clock.clone(),
            inbox.clone(),
            EyesConfig::DEFAULT,
            SplitMix64::new(1),
        );
        Rig {
            app,
            matrix,
            clock,
            inbox,
        }
    }

    impl Rig {
        fn frame_at(&mut self, now_ms: u64) {
            self.clock.set(now_ms);
            self.app.frame();
        }
    }

    #[test]
    fn each_frame_draws_and_presents_once() {
        let mut rig = rig();
        rig.frame_at(0);
        assert_eq!(
            (rig.matrix.flushes(), rig.matrix.lit_presented() > 0),
            (1, true)
        );
    }

    #[test]
    fn distance_messages_drive_tracking() {
        let cases: [(&[u8], Mode); 4] = [
            (b"1.83", Mode::Tracking),
            (b"4.50", Mode::Idle),
            (b"none", Mode::Idle),
            (b"garbage", Mode::Idle),
        ];
        for (payload, want) in cases {
            let mut rig = rig();
            // Two readings: the eyes need consecutive sightings before they notice someone.
            rig.inbox.push(topics::DISTANCE, payload);
            rig.inbox.push(topics::DISTANCE, payload);
            rig.frame_at(100);
            assert_eq!(rig.app.mode(), want, "{:?}", core::str::from_utf8(payload));
        }
    }

    #[test]
    fn bad_payloads_still_render_a_frame() {
        let mut rig = rig();
        rig.inbox.push(topics::DISTANCE, &[0xFF, 0x00, 0xFE]);
        rig.inbox.push(topics::CONFIG, b"{not json");
        rig.frame_at(100);
        assert_eq!(rig.matrix.flushes(), 1);
    }

    #[test]
    fn unknown_topics_are_ignored() {
        let mut rig = rig();
        rig.inbox.push("halloween/other", b"1.0");
        rig.frame_at(100);
        assert_eq!(rig.app.mode(), Mode::Idle);
    }

    #[test]
    fn config_messages_merge_over_the_defaults_not_the_previous_message() {
        let mut rig = rig();
        rig.inbox.push(topics::CONFIG, br#"{"eyes_height_m": 2.2}"#);
        rig.inbox.push(topics::CONFIG, br#"{"face_height_m": 1.2}"#);
        rig.frame_at(100);
        let expected = EyesConfig {
            face_height_m: 1.2,
            ..EyesConfig::DEFAULT
        };
        assert_eq!(*rig.app.config(), expected);
    }

    #[test]
    fn invalid_config_keeps_the_current_one() {
        let mut rig = rig();
        rig.inbox.push(topics::CONFIG, br#"{"eyes_height_m": 2.2}"#);
        rig.inbox.push(topics::CONFIG, br#"{"eyes_height_m": 99}"#);
        rig.frame_at(100);
        assert_eq!(rig.app.config().eyes_height_m, 2.2);
    }

    #[test]
    fn clearing_the_retained_config_restores_defaults() {
        let mut rig = rig();
        rig.inbox.push(topics::CONFIG, br#"{"eyes_height_m": 2.2}"#);
        rig.inbox.push(topics::CONFIG, b"");
        rig.frame_at(100);
        assert_eq!(*rig.app.config(), EyesConfig::DEFAULT);
    }

    #[test]
    fn brightness_is_written_at_start_and_only_when_it_changes() {
        let mut rig = rig();
        rig.inbox.push(topics::CONFIG, br#"{"brightness": 30}"#);
        rig.frame_at(100);
        rig.inbox.push(
            topics::CONFIG,
            br#"{"brightness": 30, "face_height_m": 1.5}"#,
        );
        rig.frame_at(200);
        rig.frame_at(300);
        assert_eq!(
            rig.matrix.brightness_writes(),
            [EyesConfig::DEFAULT.brightness, 30]
        );
    }

    #[test]
    fn a_message_flood_is_spread_across_frames() {
        let mut rig = rig();
        for _ in 0..MAX_MESSAGES_PER_FRAME + 5 {
            rig.inbox.push(topics::DISTANCE, b"2.0");
        }
        rig.frame_at(100);
        assert_eq!(rig.inbox.pending(), 5);
    }

    #[test]
    fn readings_go_stale_without_new_messages() {
        let mut rig = rig();
        rig.inbox.push(topics::DISTANCE, b"2.0");
        rig.frame_at(100);
        rig.frame_at(100 + 2_001);
        assert_eq!(rig.app.mode(), Mode::Idle);
    }

    #[test]
    fn printable_truncates_and_survives_binary() {
        let cases: [(&[u8], &str); 3] = [
            (b"1.83", "1.83"),
            (&[b'a'; 40], "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            (&[0xFF], "<not UTF-8>"),
        ];
        for (payload, want) in cases {
            assert_eq!(printable(payload), want);
        }
    }
}

//! The eyes' behaviour: track the visitor when there is one, otherwise wander; blink either way.
//!
//! Inputs arrive as [`Eyes::observe`] (a sensor reading, ~10 Hz, noisy) and [`Eyes::tick`] (once per
//! rendered frame). Both take the current time in milliseconds from a monotonic clock owned by the caller.

use crate::blink::Blink;
use crate::config::EyesConfig;
use crate::geometry::{gaze_angles, gaze_point, gaze_shift_deg, GazeAngles, Vec2};
use crate::motion::{approach, Spring};
use crate::payload::Reading;
use crate::rng::Rng;

/// Spring speed while tracking: ~90% of a step in 0.28 s. Fast enough to feel attentive, slow enough to
/// hide the 10 Hz steps and jitter of the sensor.
const TRACK_OMEGA: f32 = 14.0;
/// Lazier spring for idle wandering.
const IDLE_OMEGA: f32 = 6.0;
/// A frame that took longer than this (a stalled task, a background browser tab) is treated as this long,
/// so timers resume rather than skipping whole blinks.
const MAX_DT_S: f32 = 0.25;

const WANDER_INTERVAL_S: (f32, f32) = (1.0, 3.5);
const WANDER_REACH: Vec2 = Vec2::new(0.75, 0.35);
/// Chance that a wander glance returns to straight ahead, so the eyes don't look permanently shifty.
const WANDER_CENTRE_CHANCE: f32 = 0.25;
/// After a visitor leaves, keep staring where they were for a moment before wandering off.
const LINGER_MS: u64 = 1500;

/// Debounce for the ultrasonic sensor's dropouts: a visitor is only noticed after this many consecutive
/// in-range readings, and only let go after "nobody" has been reported continuously for `LEAVE_MS`.
/// Without it, alternating hits and misses flip the mode (and fire a startled blink) every 100 ms.
const ENTER_READINGS: u32 = 2;
/// Sightings further apart than this (2.5 sensor periods) aren't consecutive: the count starts again, so
/// a lone reading after a silence can't complete a streak begun long before.
const MAX_SIGHTING_GAP_MS: u64 = 250;
const LEAVE_MS: u64 = 500;
/// At most one startled blink per this long, however often the visitor comes and goes.
const STARTLE_COOLDOWN_MS: u64 = 3000;

/// Idle gaze shifts at least this large are saccades and carry a coupled blink (see [`crate::blink`]);
/// smaller ones are corrective glances, which people make without blinking. Gaze-evoked blinks become
/// likely for head-fixed saccades of roughly 10 degrees and up. Capped at 40% of `max_gaze_deg` so a
/// narrow gaze range still produces blinks.
const SACCADE_THRESHOLD_DEG: f32 = 10.0;
const SACCADE_THRESHOLD_OF_MAX_GAZE: f32 = 0.4;
/// Idle eyes never go longer than this without a blink: when it runs out, the next glance is forced to
/// be a saccade (which carries one). People blink every few seconds.
const MAX_DWELL_MS: u64 = 6000;
/// A glance takes about 0.8 s to settle on the idle spring; a forced saccade waits at most this long for
/// it, so the dwell limit stays close to `MAX_DWELL_MS`.
const FORCED_SACCADE_MAX_WAIT_MS: u64 = 800;
/// A forced saccade jumps to the other side, at least this far from centre.
const FORCED_SACCADE_REACH: (f32, f32) = (0.45, 0.75);

/// Routine blinks while tracking, deferred while the gaze is moving.
const TRACKING_BLINK_INTERVAL_S: (f32, f32) = (2.0, 5.0);
const DOUBLE_BLINK_CHANCE: f32 = 0.15;
const DOUBLE_BLINK_GAP_MS: u64 = 150;
/// The gaze counts as moving while either eye's spring is faster than this (unit disc per second) or
/// further than `MOVING_ERROR` from its target. The sensor's jitter at walking distances moves the target
/// by well under both, so steady tracking still counts as settled.
const MOVING_SPEED: f32 = 0.2;
const MOVING_ERROR: f32 = 0.04;

/// Distance at which the pupils are fully dilated.
const NEAR_M: f32 = 0.3;
const IDLE_DILATION: f32 = 0.35;
const DILATION_TAU_S: f32 = 0.35;
/// The eyes start narrowing suspiciously inside this distance and are fully narrowed at `SQUINT_FULL_M`.
const SQUINT_START_M: f32 = 1.0;
const SQUINT_FULL_M: f32 = 0.4;
const SQUINT_MAX: f32 = 0.4;
const SQUINT_TAU_S: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Tracking,
}

/// Everything the renderer needs for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Per-eye gaze in the unit disc, screen-left eye first.
    pub gaze: [Vec2; 2],
    /// 0 = open, 1 = shut.
    pub blink: f32,
    /// 0 = relaxed, up to [`SQUINT_MAX`] when the visitor is very close.
    pub squint: f32,
    /// Pupil size, 0 = pinpoint, 1 = fully dilated.
    pub dilation: f32,
}

impl Pose {
    pub const RESTING: Self = Self {
        gaze: [Vec2::ZERO; 2],
        blink: 0.0,
        squint: 0.0,
        dilation: IDLE_DILATION,
    };
}

/// What the sensor has said recently, debounced into "is someone there".
#[derive(Clone, Copy, Debug, Default)]
struct Presence {
    /// The most recent in-range reading and when it arrived.
    last_seen: Option<(f32, u64)>,
    /// Consecutive in-range readings up to now.
    streak: u32,
    /// When "nobody" started being reported after the last in-range reading.
    absent_since_ms: Option<u64>,
}

pub struct Eyes<R: Rng> {
    config: EyesConfig,
    rng: R,
    mode: Mode,
    last_tick_ms: u64,
    presence: Presence,
    last_startle_ms: Option<u64>,
    /// Last gaze targets aimed at the current visitor while in front of the panel; held while their face
    /// is behind it. Cleared when they leave, so the next visitor never inherits it.
    held_targets: Option<[Vec2; 2]>,
    springs: [Spring; 2],
    wander_target: Vec2,
    next_wander_ms: u64,
    blink: Option<Blink>,
    last_blink_ms: u64,
    next_routine_blink_ms: u64,
    moving: bool,
    dilation: f32,
    squint: f32,
}

impl<R: Rng> Eyes<R> {
    pub fn new(config: EyesConfig, rng: R, now_ms: u64) -> Self {
        Self {
            config,
            rng,
            mode: Mode::Idle,
            last_tick_ms: now_ms,
            presence: Presence::default(),
            last_startle_ms: None,
            held_targets: None,
            springs: [Spring::default(); 2],
            wander_target: Vec2::ZERO,
            next_wander_ms: now_ms + LINGER_MS,
            blink: None,
            last_blink_ms: now_ms,
            next_routine_blink_ms: now_ms,
            moving: false,
            dilation: IDLE_DILATION,
            squint: 0.0,
        }
    }
    pub fn config(&self) -> &EyesConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: EyesConfig) {
        self.config = config;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether the gaze was still travelling at the last tick.
    pub fn gaze_moving(&self) -> bool {
        self.moving
    }

    pub fn observe(&mut self, reading: Reading, now_ms: u64) {
        let presence = &mut self.presence;
        match reading {
            Reading::Distance(metres) if metres < self.config.max_range_m => {
                let continues = presence.last_seen.is_some_and(|(_, seen_ms)| {
                    now_ms.saturating_sub(seen_ms) <= MAX_SIGHTING_GAP_MS
                });
                presence.streak = if continues {
                    presence.streak.saturating_add(1)
                } else {
                    1
                };
                presence.last_seen = Some((metres, now_ms));
                presence.absent_since_ms = None;
            }
            _ => {
                presence.streak = 0;
                presence.absent_since_ms.get_or_insert(now_ms);
            }
        }
    }

    /// The visitor's distance, debounced: while tracking, the last in-range reading is held through
    /// dropouts shorter than `LEAVE_MS` and silences shorter than the stale timeout.
    pub fn visitor_distance(&self, now_ms: u64) -> Option<f32> {
        let (distance_m, seen_ms) = self.presence.last_seen?;
        let fresh = now_ms.saturating_sub(seen_ms) as f32 / 1000.0 <= self.config.stale_timeout_s;
        let absent_ms = self
            .presence
            .absent_since_ms
            .map(|since_ms| now_ms.saturating_sub(since_ms));
        let present = match self.mode {
            Mode::Idle => self.presence.streak >= ENTER_READINGS,
            Mode::Tracking => absent_ms.is_none_or(|absent_ms| absent_ms < LEAVE_MS),
        };
        (fresh && present && distance_m < self.config.max_range_m).then_some(distance_m)
    }

    pub fn tick(&mut self, now_ms: u64) -> Pose {
        // Clocks are monotonic, but if one ever steps back, freeze rather than replay or panic.
        let now_ms = now_ms.max(self.last_tick_ms);
        let dt_s = ((now_ms - self.last_tick_ms) as f32 / 1000.0).min(MAX_DT_S);
        self.last_tick_ms = now_ms;

        // A visitor the eyes have nothing sensible to aim at (behind the panel, never seen in front of
        // it) counts as nobody.
        let tracked = self
            .visitor_distance(now_ms)
            .and_then(|distance_m| Some((distance_m, self.tracking_targets(distance_m)?)));
        self.update_mode(tracked.map(|(_, targets)| targets), now_ms);
        let distance_m = tracked.map(|(distance_m, _)| distance_m);

        let (targets, omega) = match tracked {
            Some((_, targets)) => (targets, TRACK_OMEGA),
            None => ([self.wander(now_ms); 2], IDLE_OMEGA),
        };
        for (spring, target) in self.springs.iter_mut().zip(targets) {
            spring.step(target, omega, dt_s);
        }
        self.moving = self.springs.iter().zip(targets).any(|(spring, target)| {
            spring.velocity.length() > MOVING_SPEED
                || (target - spring.position).length() > MOVING_ERROR
        });

        self.dilation = approach(
            self.dilation,
            self.dilation_target(distance_m),
            DILATION_TAU_S,
            dt_s,
        );
        self.squint = approach(self.squint, squint_target(distance_m), SQUINT_TAU_S, dt_s);

        Pose {
            gaze: self
                .springs
                .map(|spring| spring.position.clamped_to_unit_disc()),
            blink: self.blink_closure(now_ms),
            squint: self.squint,
            dilation: self.dilation,
        }
    }

    /// `tracking_targets` is where the eyes will aim if a visitor is being tracked.
    fn update_mode(&mut self, tracking_targets: Option<[Vec2; 2]>, now_ms: u64) {
        let mode = if tracking_targets.is_some() {
            Mode::Tracking
        } else {
            Mode::Idle
        };
        match (self.mode, mode) {
            (Mode::Idle, Mode::Tracking) => {
                let startle_due = self
                    .last_startle_ms
                    .is_none_or(|last_ms| now_ms.saturating_sub(last_ms) >= STARTLE_COOLDOWN_MS);
                // The acquisition move is a saccade, so its blink is coupled to the move. A blink already
                // under way (an idle saccade's, on a slower spring) is retimed to land with the tracking
                // gaze instead of still being shut after the eyes have arrived.
                let targets = tracking_targets.unwrap_or_default();
                let acquisition = Blink::during_move(now_ms, self.springs, targets, TRACK_OMEGA);
                match self.blink {
                    Some(blink) if !blink.is_over(now_ms) => {
                        self.blink = Some(blink.retimed_to(now_ms, acquisition));
                    }
                    _ if startle_due => {
                        self.start_blink(acquisition);
                        self.last_startle_ms = Some(now_ms);
                    }
                    _ => {}
                }
                self.schedule_routine_blink(now_ms, false);
            }
            (Mode::Tracking, Mode::Idle) => {
                self.held_targets = None;
                self.wander_target = self.gaze_centre();
                self.next_wander_ms = now_ms + LINGER_MS;
            }
            _ => {}
        }
        self.mode = mode;
    }

    /// Where each eye should look at a visitor `distance_m` away. A face behind the panel's plane can't be
    /// looked at (the angles swing past 90 degrees and the eyes would cross), so the last sensible targets
    /// are held instead, or `None` if there are none yet.
    fn tracking_targets(&mut self, distance_m: f32) -> Option<[Vec2; 2]> {
        let angles = gaze_angles(&self.config, distance_m);
        if !angles.iter().any(GazeAngles::is_behind) {
            let max_gaze_deg = self.config.max_gaze_deg;
            self.held_targets = Some(angles.map(|angles| gaze_point(angles, max_gaze_deg)));
        }
        self.held_targets
    }

    fn wander(&mut self, now_ms: u64) -> Vec2 {
        if now_ms >= self.next_wander_ms {
            let dwell_over = now_ms >= self.last_blink_ms + MAX_DWELL_MS;
            // The coupled blink's timeline assumes the spring starts from rest, so a forced saccade waits
            // for the previous glance to settle, but never more than `FORCED_SACCADE_MAX_WAIT_MS`.
            let may_wait = now_ms < self.last_blink_ms + MAX_DWELL_MS + FORCED_SACCADE_MAX_WAIT_MS;
            if dwell_over && self.moving && may_wait {
                return self.wander_target;
            }
            let target = if dwell_over {
                self.forced_saccade_target()
            } else if self.rng.unit() < WANDER_CENTRE_CHANCE {
                Vec2::ZERO
            } else {
                Vec2::new(
                    self.rng.between(-WANDER_REACH.x, WANDER_REACH.x),
                    self.rng.between(-WANDER_REACH.y, WANDER_REACH.y),
                )
            };
            self.shift_gaze(target, now_ms);
            let interval_ms =
                seconds_to_ms(self.rng.between(WANDER_INTERVAL_S.0, WANDER_INTERVAL_S.1));
            self.next_wander_ms = (now_ms + interval_ms).min(self.last_blink_ms + MAX_DWELL_MS);
        }
        self.wander_target
    }

    fn forced_saccade_target(&mut self) -> Vec2 {
        let side = if self.gaze_centre().x >= 0.0 {
            -1.0
        } else {
            1.0
        };
        Vec2::new(
            side * self
                .rng
                .between(FORCED_SACCADE_REACH.0, FORCED_SACCADE_REACH.1),
            self.rng.between(-WANDER_REACH.y, WANDER_REACH.y),
        )
    }

    /// Start an idle glance towards `target`; a large enough one is a saccade with a coupled blink.
    fn shift_gaze(&mut self, target: Vec2, now_ms: u64) {
        let max_gaze_deg = self.config.max_gaze_deg;
        let threshold_deg = SACCADE_THRESHOLD_DEG.min(max_gaze_deg * SACCADE_THRESHOLD_OF_MAX_GAZE);
        let is_saccade = gaze_shift_deg(self.gaze_centre(), target, max_gaze_deg) >= threshold_deg;
        if is_saccade && self.blink.is_none() {
            self.start_blink(Blink::during_move(
                now_ms,
                self.springs,
                [target; 2],
                IDLE_OMEGA,
            ));
        }
        self.wander_target = target;
    }

    fn gaze_centre(&self) -> Vec2 {
        (self.springs[0].position + self.springs[1].position) * 0.5
    }

    fn start_blink(&mut self, blink: Blink) {
        self.blink = Some(blink);
        self.last_blink_ms = blink.start_ms;
    }

    /// A double blink may only follow a routine one: right after a coupled blink the gaze may still be
    /// finishing its move.
    fn schedule_routine_blink(&mut self, now_ms: u64, after_routine: bool) {
        let gap_ms = if after_routine && self.rng.unit() < DOUBLE_BLINK_CHANCE {
            DOUBLE_BLINK_GAP_MS
        } else {
            seconds_to_ms(
                self.rng
                    .between(TRACKING_BLINK_INTERVAL_S.0, TRACKING_BLINK_INTERVAL_S.1),
            )
        };
        self.next_routine_blink_ms = now_ms + gap_ms;
    }

    /// Idle eyes only blink with saccades. Tracking eyes also blink routinely, but never mid-move: a due
    /// blink waits until the gaze has settled.
    fn blink_closure(&mut self, now_ms: u64) -> f32 {
        if let Some(blink) = self.blink.filter(|blink| blink.is_over(now_ms)) {
            self.blink = None;
            if self.mode == Mode::Tracking {
                self.schedule_routine_blink(now_ms, blink == Blink::routine(blink.start_ms));
            }
        }
        let routine_due = self.mode == Mode::Tracking
            && self.blink.is_none()
            && now_ms >= self.next_routine_blink_ms
            && !self.moving;
        if routine_due {
            self.start_blink(Blink::routine(now_ms));
        }
        self.blink.map_or(0.0, |blink| blink.closure(now_ms))
    }

    fn dilation_target(&self, distance_m: Option<f32>) -> f32 {
        match distance_m {
            Some(distance_m) => {
                let span = (self.config.max_range_m - NEAR_M).max(f32::EPSILON);
                (1.0 - (distance_m - NEAR_M) / span).clamp(0.0, 1.0)
            }
            None => IDLE_DILATION,
        }
    }
}

fn squint_target(distance_m: Option<f32>) -> f32 {
    match distance_m {
        Some(distance_m) => {
            SQUINT_MAX
                * ((SQUINT_START_M - distance_m) / (SQUINT_START_M - SQUINT_FULL_M)).clamp(0.0, 1.0)
        }
        None => 0.0,
    }
}

fn seconds_to_ms(seconds: f32) -> u64 {
    (seconds * 1000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Param;
    use crate::rng::SplitMix64;
    use alloc::vec::Vec;

    const FRAME_MS: u64 = 33;

    fn eyes() -> Eyes<SplitMix64> {
        Eyes::new(EyesConfig::DEFAULT, SplitMix64::new(1), 0)
    }

    /// Ticks every frame from `from_ms` (exclusive) to `to_ms` (inclusive), feeding `reading` every 100 ms
    /// like the sensor does, and returns every pose.
    fn run(
        eyes: &mut Eyes<SplitMix64>,
        reading: Option<Reading>,
        from_ms: u64,
        to_ms: u64,
    ) -> Vec<Pose> {
        let mut poses = Vec::new();
        let mut now_ms = from_ms;
        while now_ms < to_ms {
            let next_ms = (now_ms + FRAME_MS).min(to_ms);
            if let Some(reading) = reading {
                if next_ms / 100 != now_ms / 100 || now_ms == from_ms {
                    eyes.observe(reading, next_ms);
                }
            }
            poses.push(eyes.tick(next_ms));
            now_ms = next_ms;
        }
        poses
    }

    fn expected_gaze(config: &EyesConfig, distance_m: f32) -> [Vec2; 2] {
        gaze_angles(config, distance_m).map(|angles| gaze_point(angles, config.max_gaze_deg))
    }

    fn blinks(poses: &[Pose]) -> usize {
        poses
            .windows(2)
            .filter(|pair| pair[0].blink < 0.9 && pair[1].blink >= 0.9)
            .count()
    }

    #[test]
    fn idle_eyes_notice_a_visitor_only_after_consecutive_readings() {
        const HIT: Reading = Reading::Distance(2.0);
        /// Readings with their arrival times.
        type Script = &'static [(Reading, u64)];
        let cases: [(&str, Script, u64, Mode); 7] = [
            (
                "one reading is not enough",
                &[(HIT, 1000)],
                1000,
                Mode::Idle,
            ),
            (
                "two in a row",
                &[(HIT, 900), (HIT, 1000)],
                1000,
                Mode::Tracking,
            ),
            (
                "a miss in between restarts the count",
                &[(HIT, 800), (Reading::Nobody, 900), (HIT, 1000)],
                1000,
                Mode::Idle,
            ),
            (
                "porch-sensor's old nobody value",
                &[
                    (Reading::Distance(4.5), 900),
                    (Reading::Distance(4.5), 1000),
                ],
                1000,
                Mode::Idle,
            ),
            (
                "literal none",
                &[(Reading::Nobody, 900), (Reading::Nobody, 1000)],
                1000,
                Mode::Idle,
            ),
            (
                "just inside the stale timeout",
                &[(HIT, 900), (HIT, 1000)],
                3000,
                Mode::Tracking,
            ),
            ("stale", &[(HIT, 900), (HIT, 1000)], 3001, Mode::Idle),
        ];
        for (name, observations, ticked_at_ms, want) in cases {
            let mut eyes = eyes();
            for &(reading, at_ms) in observations {
                eyes.observe(reading, at_ms);
            }
            eyes.tick(ticked_at_ms);
            assert_eq!(eyes.mode(), want, "{name}");
        }
    }

    /// Starts tracking a visitor at 2 m, then reports `nobody` from `absent_from_ms` and ticks at `now_ms`.
    fn tracking_then_absent(absent_from_ms: u64, now_ms: u64) -> Mode {
        let mut eyes = eyes();
        run(&mut eyes, Some(Reading::Distance(2.0)), 0, absent_from_ms);
        eyes.observe(Reading::Nobody, absent_from_ms);
        eyes.tick(now_ms);
        eyes.mode()
    }

    #[test]
    fn tracking_survives_a_short_dropout() {
        assert_eq!(
            tracking_then_absent(1000, 1000 + LEAVE_MS - 1),
            Mode::Tracking
        );
    }

    #[test]
    fn tracking_ends_after_a_sustained_absence() {
        assert_eq!(tracking_then_absent(1000, 1000 + LEAVE_MS), Mode::Idle);
    }

    /// Feeds a repeating pattern of readings (true = visitor at 2 m, false = porch-sensor's `none`) every
    /// 100 ms for 10 s and counts blinks.
    fn blinks_for_pattern(pattern: &[bool]) -> usize {
        let mut eyes = eyes();
        let mut poses = Vec::new();
        let mut reading_index = 0;
        for now_ms in (0..=10_000).step_by(FRAME_MS as usize) {
            while reading_index * 100 <= now_ms as usize {
                let hit = pattern[reading_index % pattern.len()];
                let reading = if hit {
                    Reading::Distance(2.0)
                } else {
                    Reading::Nobody
                };
                eyes.observe(reading, (reading_index * 100) as u64);
                reading_index += 1;
            }
            poses.push(eyes.tick(now_ms));
        }
        blinks(&poses)
    }

    #[test]
    fn flaky_readings_do_not_make_the_eyes_flutter() {
        let steady = blinks_for_pattern(&[true]);
        let patterns: [(&str, &[bool]); 5] = [
            ("alternating hit/miss", &[true, false]),
            ("two hits, one miss", &[true, true, false]),
            ("hit then three misses", &[true, false, false, false]),
            (
                "two hits then five misses",
                &[true, true, false, false, false, false, false],
            ),
            (
                "half-second on/off",
                &[
                    true, true, true, true, true, false, false, false, false, false,
                ],
            ),
        ];
        for (name, pattern) in patterns {
            let count = blinks_for_pattern(pattern);
            // Ordinary blinking plus at most one startle per cooldown period.
            let limit = steady + (10_000 / STARTLE_COOLDOWN_MS) as usize + 1;
            assert!(
                count <= limit,
                "{name}: {count} blinks in 10 s, steady tracking has {steady}"
            );
        }
    }

    #[test]
    fn startled_blinks_are_rate_limited() {
        let mut eyes = eyes();
        let mut startles = Vec::new();
        // A visitor appears (two hits), then is reported gone for long enough to go idle, every 0.9 s.
        for cycle in 0..10u64 {
            let start_ms = cycle * 900;
            eyes.observe(Reading::Distance(2.0), start_ms);
            eyes.observe(Reading::Distance(2.0), start_ms + 100);
            eyes.tick(start_ms + 100);
            eyes.observe(Reading::Nobody, start_ms + 200);
            eyes.tick(start_ms + 200 + LEAVE_MS);
            if eyes.last_startle_ms != startles.last().copied() {
                startles.extend(eyes.last_startle_ms);
            }
        }
        assert_eq!(startles, [100, 3700, 7300]);
    }

    /// Idle eyes resting at centre with wandering switched off, then one glance to `target` at t = 1000.
    /// Returns (progress of the move, lid closure) every 5 ms for 1.5 s.
    fn idle_shift(target: Vec2) -> Vec<(f32, f32)> {
        let mut eyes = eyes();
        eyes.next_wander_ms = u64::MAX;
        eyes.tick(1000);
        eyes.shift_gaze(target, 1000);
        (1000..2500)
            .step_by(5)
            .map(|now_ms| {
                let pose = eyes.tick(now_ms);
                (pose.gaze[0].x / target.x, pose.blink)
            })
            .collect()
    }

    const LARGE_SHIFT: Vec2 = Vec2::new(0.7, 0.0);

    #[test]
    fn lid_is_shut_at_mid_movement_of_a_large_idle_shift() {
        let samples = idle_shift(LARGE_SHIFT);
        let (_, closure) = samples
            .iter()
            .find(|(progress, _)| *progress >= 0.5)
            .unwrap();
        assert!(*closure >= 0.9, "closure {closure} at mid-movement");
    }

    #[test]
    fn lid_is_open_again_before_the_gaze_settles() {
        let samples = idle_shift(LARGE_SHIFT);
        let settled = samples
            .iter()
            .position(|(progress, _)| *progress >= 0.95)
            .unwrap();
        let last_closed = samples
            .iter()
            .rposition(|(_, closure)| *closure > 0.0)
            .unwrap();
        assert!(
            last_closed < settled,
            "lid still closing at sample {last_closed}, settled at {settled}"
        );
    }

    #[test]
    fn lid_starts_closing_only_after_the_gaze_starts_moving() {
        let samples = idle_shift(LARGE_SHIFT);
        let (progress, _) = samples.iter().find(|(_, closure)| *closure > 0.0).unwrap();
        assert!(*progress > 0.05, "lid moved at progress {progress}");
    }

    #[test]
    fn a_small_glance_does_not_blink() {
        let samples = idle_shift(Vec2::new(0.1, 0.0));
        assert!(samples.iter().all(|(_, closure)| *closure == 0.0));
    }

    #[test]
    fn tracking_defers_a_due_blink_until_the_gaze_settles() {
        let mut eyes = eyes();
        run(&mut eyes, Some(Reading::Distance(3.0)), 0, 3000);
        eyes.blink = None;
        eyes.next_routine_blink_ms = 3000;
        // The visitor jumps close: a big move, during which the due blink must wait.
        let samples: Vec<(bool, f32)> = (3000..5000)
            .step_by(10)
            .map(|now_ms| {
                if now_ms % 100 == 0 {
                    eyes.observe(Reading::Distance(0.4), now_ms);
                }
                let pose = eyes.tick(now_ms);
                (eyes.gaze_moving(), pose.blink)
            })
            .collect();
        let first_blink = samples
            .iter()
            .position(|(_, closure)| *closure > 0.0)
            .unwrap();
        let moved = samples[..first_blink].iter().any(|(moving, _)| *moving);
        let settled_when_blinking = !samples[first_blink].0;
        assert!(
            moved && settled_when_blinking,
            "first blink at sample {first_blink}"
        );
    }

    #[test]
    fn acquiring_a_visitor_blinks_with_the_move() {
        let mut eyes = eyes();
        eyes.observe(Reading::Distance(1.0), 900);
        eyes.observe(Reading::Distance(1.0), 1000);
        eyes.tick(1000);
        let targets = expected_gaze(eyes.config(), 1.0);
        let coupled = Blink::during_move(1000, [Spring::default(); 2], targets, TRACK_OMEGA);
        assert_eq!(eyes.blink, Some(coupled));
    }

    /// Ticks idle eyes for a minute and returns, for each blink, whether a new glance started on the same
    /// tick, plus every blink's start time.
    fn idle_minute() -> (Vec<bool>, Vec<u64>) {
        let mut eyes = eyes();
        let (mut with_shift, mut starts) = (Vec::new(), Vec::new());
        for now_ms in (0..60_000).step_by(FRAME_MS as usize) {
            let (blink_before, target_before) = (eyes.blink, eyes.wander_target);
            eyes.tick(now_ms);
            if eyes.blink.is_some() && eyes.blink != blink_before {
                with_shift.push(eyes.wander_target != target_before);
                starts.extend(eyes.blink.map(|blink| blink.start_ms));
            }
        }
        (with_shift, starts)
    }

    #[test]
    fn idle_eyes_only_blink_with_a_glance() {
        let (with_shift, _) = idle_minute();
        assert!(!with_shift.is_empty() && with_shift.iter().all(|&shifted| shifted));
    }

    #[test]
    fn idle_eyes_never_go_longer_than_max_dwell_without_blinking() {
        let (_, starts) = idle_minute();
        let longest = starts
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .max()
            .unwrap();
        assert!(
            longest <= MAX_DWELL_MS + FRAME_MS,
            "{longest} ms between blinks"
        );
    }

    /// Idle eyes start a large glance (with its coupled blink) at t = 1000; a visitor 1.5 m away is then
    /// acquired at `acquire_ms`. Returns every 5 ms from acquisition: (progress of the tracking move,
    /// lid closure).
    fn acquire_during_idle_blink(acquire_ms: u64) -> Vec<(f32, f32)> {
        let mut eyes = eyes();
        eyes.next_wander_ms = u64::MAX;
        eyes.tick(1000);
        eyes.shift_gaze(Vec2::new(-0.7, 0.0), 1000);
        (1000..acquire_ms).step_by(5).for_each(|now_ms| {
            eyes.tick(now_ms);
        });
        let from = eyes.springs[0].position;
        let to = expected_gaze(eyes.config(), 1.5)[0];
        eyes.observe(Reading::Distance(1.5), acquire_ms - 100);
        eyes.observe(Reading::Distance(1.5), acquire_ms);
        (acquire_ms..acquire_ms + 1000)
            .step_by(5)
            .map(|now_ms| {
                if now_ms % 100 == 0 {
                    eyes.observe(Reading::Distance(1.5), now_ms);
                }
                let pose = eyes.tick(now_ms);
                let moved = pose.gaze[0] - from;
                let whole = to - from;
                let progress = (moved.x * whole.x + moved.y * whole.y)
                    / (whole.x * whole.x + whole.y * whole.y);
                (progress, pose.blink)
            })
            .collect()
    }

    /// Acquisition while the idle blink is closing, at its shut instant, and reopening.
    const IDLE_BLINK_PHASES_MS: [(&str, u64); 3] =
        [("closing", 1180), ("shut", 1269), ("reopening", 1400)];

    #[test]
    fn acquiring_mid_idle_blink_opens_the_lid_before_the_tracking_gaze_lands() {
        for (phase, acquire_ms) in IDLE_BLINK_PHASES_MS {
            let samples = acquire_during_idle_blink(acquire_ms);
            let landed = samples
                .iter()
                .position(|(progress, _)| *progress >= 0.95)
                .unwrap();
            let last_closed = samples
                .iter()
                .rposition(|(_, closure)| *closure > 0.0)
                .unwrap();
            assert!(
                last_closed < landed,
                "{phase}: lid open at sample {last_closed}, landed at {landed}"
            );
        }
    }

    #[test]
    fn acquiring_mid_idle_blink_never_makes_the_lid_jump() {
        // The fastest legitimate lid motion is a routine blink closing in 70 ms: 5/70 per 5 ms frame.
        for (phase, acquire_ms) in IDLE_BLINK_PHASES_MS {
            let samples = acquire_during_idle_blink(acquire_ms);
            let largest = samples
                .windows(2)
                .map(|pair| (pair[1].1 - pair[0].1).abs())
                .fold(0.0, f32::max);
            assert!(largest <= 0.08, "{phase}: lid moved {largest} in one frame");
        }
    }

    #[test]
    fn no_double_blink_follows_an_acquisition_blink() {
        // Across many seeds (15% of which would draw a double), the blink after the acquisition one still
        // waits the ordinary 2-5 s.
        for seed in 1..40 {
            let mut eyes = Eyes::new(EyesConfig::DEFAULT, SplitMix64::new(seed), 0);
            let poses = run(&mut eyes, Some(Reading::Distance(1.5)), 0, 2000);
            assert!(
                blinks(&poses) <= 1,
                "seed {seed}: {} blinks in the first 2 s",
                blinks(&poses)
            );
        }
    }

    #[test]
    fn a_forced_saccade_waits_for_the_previous_glance_to_settle() {
        let mut eyes = eyes();
        eyes.next_wander_ms = u64::MAX;
        eyes.tick(5800);
        // No blink since t = 0, and a small glance just before the dwell runs out leaves the spring moving
        // at the deadline.
        eyes.shift_gaze(Vec2::new(0.15, 0.0), 5800);
        eyes.last_blink_ms = 0;
        eyes.next_wander_ms = MAX_DWELL_MS;
        let mut forced = None;
        for now_ms in (5805..8000).step_by(5) {
            let (was_moving, target) = (eyes.gaze_moving(), eyes.wander_target);
            eyes.tick(now_ms);
            if eyes.wander_target != target {
                forced = Some((now_ms, was_moving));
                break;
            }
        }
        let (at_ms, was_moving) = forced.unwrap();
        let in_time = at_ms > MAX_DWELL_MS && at_ms <= MAX_DWELL_MS + FORCED_SACCADE_MAX_WAIT_MS;
        assert!(
            in_time && !was_moving,
            "forced at {at_ms} ms, moving {was_moving}"
        );
    }

    #[test]
    fn a_lone_reading_after_going_idle_on_silence_does_not_track() {
        let mut eyes = eyes();
        run(&mut eyes, Some(Reading::Distance(2.0)), 0, 1000);
        eyes.tick(4000);
        let went_idle = eyes.mode() == Mode::Idle;
        eyes.observe(Reading::Distance(3.0), 4100);
        eyes.tick(4100);
        assert!(went_idle && eyes.mode() == Mode::Idle);
    }

    #[test]
    fn readings_spread_further_apart_than_the_gap_are_not_consecutive() {
        let mut eyes = eyes();
        eyes.observe(Reading::Distance(2.0), 1000);
        eyes.observe(Reading::Distance(2.0), 1000 + MAX_SIGHTING_GAP_MS + 1);
        eyes.tick(1000 + MAX_SIGHTING_GAP_MS + 1);
        assert_eq!(eyes.mode(), Mode::Idle);
    }

    /// Panel 2 m down the walkway facing the approaching visitor; a visitor nearer the door than that has
    /// their face behind the panel's plane.
    fn panel_down_the_walkway() -> Eyes<SplitMix64> {
        let config = EyesConfig {
            eyes_offset_x_m: 0.5,
            eyes_offset_y_m: 2.0,
            ..EyesConfig::DEFAULT
        };
        Eyes::new(config, SplitMix64::new(1), 0)
    }

    #[test]
    fn a_face_behind_the_panel_with_nothing_held_stays_idle() {
        let mut eyes = panel_down_the_walkway();
        run(&mut eyes, Some(Reading::Distance(1.0)), 0, 2000);
        assert_eq!(eyes.mode(), Mode::Idle);
    }

    #[test]
    fn the_next_visitor_does_not_inherit_a_held_gaze() {
        let mut eyes = panel_down_the_walkway();
        run(&mut eyes, Some(Reading::Distance(3.0)), 0, 2000);
        eyes.observe(Reading::Nobody, 2000);
        run(&mut eyes, None, 2000, 5000);
        run(&mut eyes, Some(Reading::Distance(1.0)), 5000, 7000);
        assert_eq!(eyes.mode(), Mode::Idle);
    }

    #[test]
    fn a_face_behind_the_panel_holds_the_last_gaze() {
        let mut eyes = panel_down_the_walkway();
        let in_front = run(&mut eyes, Some(Reading::Distance(3.0)), 0, 2000);
        let held = run(&mut eyes, Some(Reading::Distance(1.0)), 2000, 4000);
        let before = in_front.last().unwrap().gaze;
        let after = held.last().unwrap().gaze;
        assert!(
            (before[0] - after[0]).length() < 1e-3 && (before[1] - after[1]).length() < 1e-3,
            "{before:?} -> {after:?}"
        );
    }

    #[test]
    fn starts_idle_with_no_readings() {
        let mut eyes = eyes();
        eyes.tick(10_000);
        assert_eq!(eyes.mode(), Mode::Idle);
    }

    #[test]
    fn lowering_max_range_turns_a_reading_into_nobody() {
        let mut eyes = eyes();
        eyes.set_config(EyesConfig::DEFAULT.with(Param::MaxRange, 1.5).unwrap());
        eyes.observe(Reading::Distance(2.0), 0);
        eyes.observe(Reading::Distance(2.0), 100);
        eyes.tick(100);
        assert_eq!(eyes.mode(), Mode::Idle);
    }

    #[test]
    fn tracking_settles_on_the_geometric_gaze() {
        let mut eyes = eyes();
        let poses = run(&mut eyes, Some(Reading::Distance(1.2)), 0, 2000);
        let want = expected_gaze(&EyesConfig::DEFAULT, 1.2);
        let got = poses.last().unwrap().gaze;
        assert!(
            (got[0] - want[0]).length() < 1e-3 && (got[1] - want[1]).length() < 1e-3,
            "{got:?} vs {want:?}"
        );
    }

    #[test]
    fn gaze_moves_smoothly_when_the_visitor_appears() {
        let mut eyes = eyes();
        let poses = run(&mut eyes, Some(Reading::Distance(0.5)), 0, 1000);
        let largest_step = poses
            .windows(2)
            .map(|pair| (pair[1].gaze[0] - pair[0].gaze[0]).length())
            .fold(0.0, f32::max);
        // A full sweep of the disc is 2.0; no frame may cover more than a sixth of that.
        assert!(largest_step < 0.33, "largest per-frame step {largest_step}");
    }

    #[test]
    fn eyes_converge_when_the_visitor_is_close() {
        let mut eyes = eyes();
        let poses = run(&mut eyes, Some(Reading::Distance(0.4)), 0, 2000);
        let [left, right] = poses.last().unwrap().gaze;
        assert!(left.x - right.x > 0.05, "left {left:?} right {right:?}");
    }

    #[test]
    fn blinks_while_idle() {
        let mut eyes = eyes();
        let poses = run(&mut eyes, None, 0, 30_000);
        assert!(
            (4..=15).contains(&blinks(&poses)),
            "{} blinks",
            blinks(&poses)
        );
    }

    #[test]
    fn blinks_while_tracking() {
        let mut eyes = eyes();
        let poses = run(&mut eyes, Some(Reading::Distance(2.0)), 0, 30_000);
        assert!(blinks(&poses) >= 5, "{} blinks", blinks(&poses));
    }

    #[test]
    fn the_startled_blink_fully_closes_then_reopens() {
        let mut eyes = eyes();
        eyes.observe(Reading::Distance(2.0), 0);
        eyes.observe(Reading::Distance(2.0), 0);
        let poses: Vec<Pose> = (0..=600)
            .step_by(5)
            .map(|now_ms| eyes.tick(now_ms))
            .collect();
        let peak = poses.iter().map(|pose| pose.blink).fold(0.0, f32::max);
        assert!(
            peak == 1.0 && poses.last().unwrap().blink == 0.0,
            "peak {peak}"
        );
    }

    #[test]
    fn idle_wanders_within_the_disc() {
        let mut eyes = eyes();
        let poses = run(&mut eyes, None, 0, 20_000);
        let spread = poses
            .iter()
            .map(|pose| pose.gaze[0].x)
            .fold(0.0, |widest: f32, x| widest.max(x.abs()));
        let inside = poses
            .iter()
            .all(|pose| pose.gaze.iter().all(|gaze| gaze.length() <= 1.0));
        assert!(spread > 0.2 && inside, "spread {spread}");
    }

    #[test]
    fn lingers_where_the_visitor_was_before_wandering() {
        let midpoint = |pose: &Pose| (pose.gaze[0] + pose.gaze[1]) * 0.5;
        let mut eyes = eyes();
        let tracked = run(&mut eyes, Some(Reading::Distance(1.0)), 0, 2000);
        let left_at = midpoint(tracked.last().unwrap());
        eyes.observe(Reading::Nobody, 2000);
        let lingering = run(&mut eyes, None, 2000, 2000 + LINGER_MS);
        let drift = (midpoint(lingering.last().unwrap()) - left_at).length();
        assert!(drift < 0.01, "drifted {drift} while lingering");
    }

    #[test]
    fn pupils_dilate_as_the_visitor_approaches() {
        let dilation_at = |distance_m: f32| {
            let mut eyes = eyes();
            run(&mut eyes, Some(Reading::Distance(distance_m)), 0, 3000)
                .last()
                .unwrap()
                .dilation
        };
        assert!(dilation_at(0.5) > dilation_at(2.0) + 0.3);
    }

    #[test]
    fn squints_only_when_very_close() {
        let squint_at = |distance_m: f32| {
            let mut eyes = eyes();
            run(&mut eyes, Some(Reading::Distance(distance_m)), 0, 3000)
                .last()
                .unwrap()
                .squint
        };
        assert!(squint_at(0.4) > 0.35 && squint_at(1.5) == 0.0);
    }

    #[test]
    fn same_seed_and_inputs_give_identical_poses() {
        let script = |seed| {
            let mut eyes = Eyes::new(EyesConfig::DEFAULT, SplitMix64::new(seed), 0);
            let mut poses = run(&mut eyes, None, 0, 5000);
            poses.extend(run(&mut eyes, Some(Reading::Distance(1.5)), 5000, 9000));
            poses
        };
        assert_eq!(script(7), script(7));
    }

    #[test]
    fn different_seeds_wander_differently() {
        let script = |seed| {
            let mut eyes = Eyes::new(EyesConfig::DEFAULT, SplitMix64::new(seed), 0);
            run(&mut eyes, None, 0, 10_000)
        };
        assert_ne!(script(7), script(8));
    }

    #[test]
    fn clock_stepping_backwards_freezes_instead_of_panicking() {
        let mut eyes = eyes();
        run(&mut eyes, None, 0, 4000);
        let before = eyes.tick(4000);
        let after = eyes.tick(1000);
        assert_eq!(before.gaze, after.gaze);
    }
}

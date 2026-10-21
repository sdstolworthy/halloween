//! The eyes' behaviour: track the visitor when there is one, otherwise wander; blink either way.
//!
//! Inputs arrive as [`Eyes::observe`] (a sensor reading, ~10 Hz, noisy) and [`Eyes::tick`] (once per
//! rendered frame). Both take the current time in milliseconds from a monotonic clock owned by the caller.

use crate::config::EyesConfig;
use crate::geometry::{gaze_angles, gaze_point, Vec2};
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

const BLINK_CLOSE_MS: u64 = 70;
const BLINK_HOLD_MS: u64 = 50;
const BLINK_OPEN_MS: u64 = 120;
const BLINK_MS: u64 = BLINK_CLOSE_MS + BLINK_HOLD_MS + BLINK_OPEN_MS;
const IDLE_BLINK_INTERVAL_S: (f32, f32) = (2.5, 6.0);
const TRACKING_BLINK_INTERVAL_S: (f32, f32) = (2.0, 5.0);
const DOUBLE_BLINK_CHANCE: f32 = 0.15;
const DOUBLE_BLINK_GAP_MS: u64 = 150;

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

#[derive(Clone, Copy, Debug)]
struct Observation {
    distance_m: Option<f32>,
    at_ms: u64,
}

#[derive(Clone, Copy, Debug)]
struct Blink {
    started_ms: Option<u64>,
    next_ms: u64,
}

impl Blink {
    fn closure(elapsed_ms: u64) -> f32 {
        if elapsed_ms < BLINK_CLOSE_MS {
            elapsed_ms as f32 / BLINK_CLOSE_MS as f32
        } else if elapsed_ms < BLINK_CLOSE_MS + BLINK_HOLD_MS {
            1.0
        } else {
            let opening = elapsed_ms - BLINK_CLOSE_MS - BLINK_HOLD_MS;
            1.0 - (opening as f32 / BLINK_OPEN_MS as f32).min(1.0)
        }
    }
}

pub struct Eyes<R: Rng> {
    config: EyesConfig,
    rng: R,
    mode: Mode,
    last_tick_ms: u64,
    latest: Option<Observation>,
    springs: [Spring; 2],
    wander_target: Vec2,
    next_wander_ms: u64,
    blink: Blink,
    dilation: f32,
    squint: f32,
}

impl<R: Rng> Eyes<R> {
    pub fn new(config: EyesConfig, mut rng: R, now_ms: u64) -> Self {
        let first_blink_ms =
            now_ms + seconds_to_ms(rng.between(IDLE_BLINK_INTERVAL_S.0, IDLE_BLINK_INTERVAL_S.1));
        Self {
            config,
            rng,
            mode: Mode::Idle,
            last_tick_ms: now_ms,
            latest: None,
            springs: [Spring::default(); 2],
            wander_target: Vec2::ZERO,
            next_wander_ms: now_ms + LINGER_MS,
            blink: Blink {
                started_ms: None,
                next_ms: first_blink_ms,
            },
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

    pub fn observe(&mut self, reading: Reading, now_ms: u64) {
        let distance_m = match reading {
            Reading::Distance(metres) => Some(metres),
            Reading::Nobody => None,
        };
        self.latest = Some(Observation {
            distance_m,
            at_ms: now_ms,
        });
    }

    /// The visitor's distance if the latest reading is fresh and inside the max range.
    pub fn visitor_distance(&self, now_ms: u64) -> Option<f32> {
        let latest = self.latest?;
        let age_s = now_ms.saturating_sub(latest.at_ms) as f32 / 1000.0;
        let distance_m = latest.distance_m?;
        (age_s <= self.config.stale_timeout_s && distance_m < self.config.max_range_m)
            .then_some(distance_m)
    }

    pub fn tick(&mut self, now_ms: u64) -> Pose {
        // Clocks are monotonic, but if one ever steps back, freeze rather than replay or panic.
        let now_ms = now_ms.max(self.last_tick_ms);
        let dt_s = ((now_ms - self.last_tick_ms) as f32 / 1000.0).min(MAX_DT_S);
        self.last_tick_ms = now_ms;

        let distance_m = self.visitor_distance(now_ms);
        self.update_mode(distance_m.is_some(), now_ms);

        let (targets, omega) = match distance_m {
            Some(distance_m) => {
                let max_gaze_deg = self.config.max_gaze_deg;
                let targets = gaze_angles(&self.config, distance_m)
                    .map(|angles| gaze_point(angles, max_gaze_deg));
                (targets, TRACK_OMEGA)
            }
            None => ([self.wander(now_ms); 2], IDLE_OMEGA),
        };
        for (spring, target) in self.springs.iter_mut().zip(targets) {
            spring.step(target, omega, dt_s);
        }

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

    fn update_mode(&mut self, visitor_present: bool, now_ms: u64) {
        let mode = if visitor_present {
            Mode::Tracking
        } else {
            Mode::Idle
        };
        match (self.mode, mode) {
            (Mode::Idle, Mode::Tracking) => {
                // A startled blink on noticing someone.
                if self.blink.started_ms.is_none() {
                    self.blink.next_ms = now_ms;
                }
            }
            (Mode::Tracking, Mode::Idle) => {
                self.wander_target = (self.springs[0].position + self.springs[1].position) * 0.5;
                self.next_wander_ms = now_ms + LINGER_MS;
            }
            _ => {}
        }
        self.mode = mode;
    }

    fn wander(&mut self, now_ms: u64) -> Vec2 {
        if now_ms >= self.next_wander_ms {
            self.wander_target = if self.rng.unit() < WANDER_CENTRE_CHANCE {
                Vec2::ZERO
            } else {
                Vec2::new(
                    self.rng.between(-WANDER_REACH.x, WANDER_REACH.x),
                    self.rng.between(-WANDER_REACH.y, WANDER_REACH.y),
                )
            };
            self.next_wander_ms =
                now_ms + seconds_to_ms(self.rng.between(WANDER_INTERVAL_S.0, WANDER_INTERVAL_S.1));
        }
        self.wander_target
    }

    fn blink_closure(&mut self, now_ms: u64) -> f32 {
        if let Some(started_ms) = self.blink.started_ms {
            let elapsed_ms = now_ms - started_ms;
            if elapsed_ms < BLINK_MS {
                return Blink::closure(elapsed_ms);
            }
            self.blink.started_ms = None;
        }
        if now_ms < self.blink.next_ms {
            return 0.0;
        }
        self.blink.started_ms = Some(now_ms);
        let (low, high) = match self.mode {
            Mode::Idle => IDLE_BLINK_INTERVAL_S,
            Mode::Tracking => TRACKING_BLINK_INTERVAL_S,
        };
        let gap_ms = if self.rng.unit() < DOUBLE_BLINK_CHANCE {
            DOUBLE_BLINK_GAP_MS
        } else {
            seconds_to_ms(self.rng.between(low, high))
        };
        self.blink.next_ms = now_ms + BLINK_MS + gap_ms;
        Blink::closure(0)
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
    fn mode_follows_the_latest_reading() {
        struct Case {
            name: &'static str,
            reading: Reading,
            observed_at_ms: u64,
            ticked_at_ms: u64,
            want: Mode,
        }
        let cases = [
            Case {
                name: "visitor in range",
                reading: Reading::Distance(2.0),
                observed_at_ms: 1000,
                ticked_at_ms: 1000,
                want: Mode::Tracking,
            },
            Case {
                name: "porch-sensor's nobody value",
                reading: Reading::Distance(4.5),
                observed_at_ms: 1000,
                ticked_at_ms: 1000,
                want: Mode::Idle,
            },
            Case {
                name: "literal none",
                reading: Reading::Nobody,
                observed_at_ms: 1000,
                ticked_at_ms: 1000,
                want: Mode::Idle,
            },
            Case {
                name: "just inside the stale timeout",
                reading: Reading::Distance(2.0),
                observed_at_ms: 1000,
                ticked_at_ms: 3000,
                want: Mode::Tracking,
            },
            Case {
                name: "stale reading",
                reading: Reading::Distance(2.0),
                observed_at_ms: 1000,
                ticked_at_ms: 3001,
                want: Mode::Idle,
            },
        ];
        for case in cases {
            let mut eyes = eyes();
            eyes.observe(case.reading, case.observed_at_ms);
            eyes.tick(case.ticked_at_ms);
            assert_eq!(eyes.mode(), case.want, "{}", case.name);
        }
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
        eyes.tick(0);
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
    fn blink_fully_closes_then_reopens() {
        let mut eyes = eyes();
        eyes.observe(Reading::Distance(2.0), 0);
        let poses: Vec<Pose> = (0..=BLINK_MS + 10)
            .step_by(10)
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

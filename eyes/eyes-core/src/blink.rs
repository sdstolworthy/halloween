//! Blink shapes, including blinks coupled to gaze shifts.
//!
//! Real eyes blink during large saccades: the lid starts down just after the eye starts moving, is shut
//! around mid-flight (hiding the motion blur), and is open again just before the eye lands. The gaze here
//! moves on a critically damped spring released from rest, which covers the fraction
//! `p(t) = 1 - (1 + ωt)·e^(-ωt)` of the move by time `t`. The coupled blink is scheduled at fixed points of
//! that progress curve, so it follows the spring if its stiffness ω is retuned:
//!
//! | Lid event | Move covered | ωt | at ω = 6 (idle) | at ω = 14 (tracking) |
//! | --- | --- | --- | --- | --- |
//! | starts closing | 12% | 0.59 | 98 ms | 42 ms |
//! | fully closed | 48% (just before mid-move) | 1.62 | 269 ms | 115 ms |
//! | fully open | 88% | 3.66 | 610 ms → 548 ms (clamped) | 261 ms |
//!
//! The gaze counts as settled at 95% (ωt = 4.74; 790 ms at ω = 6), so the lid is always open before it
//! lands. Human blinks last ~100–400 ms and gaze-evoked ones run longer, so the whole blink is clamped to
//! 150–450 ms, keeping its start fixed: a lazy spring gets a brisker reopening (still well before
//! settling), and a very stiff spring a blink that slightly outlasts its move rather than a twitch.

use libm::expf;

use crate::geometry::Vec2;
use crate::motion::Spring;

/// Fractions of the move at which the coupled blink starts closing, is shut, and is open again.
pub const CLOSE_START_PROGRESS: f32 = 0.12;
pub const CLOSED_PROGRESS: f32 = 0.48;
pub const OPEN_PROGRESS: f32 = 0.88;
/// The gaze is considered to have landed once this much of the move is covered.
pub const SETTLED_PROGRESS: f32 = 0.95;
pub const BLINK_DURATION_MS: (u64, u64) = (150, 450);
const PROGRESS_POINTS: [f32; 4] = [
    CLOSE_START_PROGRESS,
    CLOSED_PROGRESS,
    OPEN_PROGRESS,
    SETTLED_PROGRESS,
];
/// Moves shorter than this (unit disc) have no meaningful progress curve; they get the reference schedule.
const MIN_MOVE: f32 = 1e-3;
/// Step and horizon for following a spring forward from its actual state.
const SIMULATION_STEP_S: f32 = 0.002;
const SIMULATION_HORIZON_S: f32 = 3.0;

/// An ordinary blink while settled: quick close, brief hold, slower open.
const ROUTINE_CLOSE_MS: u64 = 70;
const ROUTINE_HOLD_MS: u64 = 50;
const ROUTINE_OPEN_MS: u64 = 120;

/// One blink's timeline in absolute milliseconds: closing from `start` to `closed`, shut until `reopen`,
/// opening until `end`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blink {
    pub start_ms: u64,
    pub closed_ms: u64,
    pub reopen_ms: u64,
    pub end_ms: u64,
}

impl Blink {
    pub fn routine(start_ms: u64) -> Self {
        let closed_ms = start_ms + ROUTINE_CLOSE_MS;
        let reopen_ms = closed_ms + ROUTINE_HOLD_MS;
        Self {
            start_ms,
            closed_ms,
            reopen_ms,
            end_ms: reopen_ms + ROUTINE_OPEN_MS,
        }
    }

    /// A blink coupled to a gaze shift that starts at `onset_ms` on a spring of stiffness `omega` released
    /// from rest (the analytic reference schedule in the table above).
    pub fn during_shift(onset_ms: u64, omega: f32) -> Self {
        let at = |progress: f32| seconds_to_ms(time_to_progress(progress, omega));
        Self::at_progress_times(onset_ms, PROGRESS_POINTS.map(at))
    }

    /// A blink coupled to the move that starts at `onset_ms` with each eye's spring in its actual state
    /// (it may already be moving) heading for its target. Uses whichever eye reaches each point first, so
    /// the lid is open before either eye lands.
    pub fn during_move(
        onset_ms: u64,
        springs: [Spring; 2],
        targets: [Vec2; 2],
        omega: f32,
    ) -> Self {
        let times = springs
            .iter()
            .zip(targets)
            .filter_map(|(spring, target)| progress_times(*spring, target, omega))
            .reduce(|a, b| core::array::from_fn(|i| a[i].min(b[i])));
        match times {
            Some(times) => Self::at_progress_times(onset_ms, times),
            None => Self::during_shift(onset_ms, omega),
        }
    }

    /// Lid events at the milliseconds after onset when the move covers [`PROGRESS_POINTS`], with the
    /// duration clamped to [`BLINK_DURATION_MS`], but never reopening after the gaze has landed.
    fn at_progress_times(onset_ms: u64, [start, closed, open, settled]: [u64; 4]) -> Self {
        let start_ms = onset_ms + start;
        let settled_ms = (onset_ms + settled).max(start_ms + 2);
        let duration_ms = (onset_ms + open)
            .saturating_sub(start_ms)
            .clamp(BLINK_DURATION_MS.0, BLINK_DURATION_MS.1);
        let end_ms = (start_ms + duration_ms).min(settled_ms - 1);
        let closed_ms = (onset_ms + closed)
            .min((start_ms + end_ms) / 2)
            .max(start_ms + 1);
        Self {
            start_ms,
            closed_ms,
            reopen_ms: closed_ms,
            end_ms,
        }
    }

    /// This blink, caught at `now_ms`, finished on `target`'s (faster) timeline: it never ends later than
    /// `target` would, and it carries on from the current closure so the lid never jumps. Closing continues
    /// to shut at `target`'s shut point; shut stays shut until then; reopening just finishes sooner.
    pub fn retimed_to(&self, now_ms: u64, target: Blink) -> Blink {
        if now_ms < self.start_ms {
            return target;
        }
        let closure = self.closure(now_ms);
        let end_ms = self.end_ms.min(target.end_ms).max(now_ms + 1);
        if now_ms < self.closed_ms {
            let closed_ms = target
                .closed_ms
                .clamp(now_ms + 1, end_ms.max(now_ms + 2) - 1);
            // Back-date the start so the closing ramp passes through the current closure now.
            let lead_ms = closure / (1.0 - closure) * (closed_ms - now_ms) as f32;
            return Blink {
                start_ms: now_ms.saturating_sub(lead_ms as u64),
                closed_ms,
                reopen_ms: closed_ms,
                end_ms: end_ms.max(closed_ms + 1),
            };
        }
        if now_ms < self.reopen_ms {
            return Blink {
                start_ms: now_ms.saturating_sub(1),
                closed_ms: now_ms,
                reopen_ms: target.closed_ms.clamp(now_ms, end_ms - 1),
                end_ms,
            };
        }
        // Reopening: stretch the opening ramp back in time so it passes through the current closure.
        let span_ms = ((end_ms - now_ms) as f32 / closure.max(f32::EPSILON)) as u64;
        let reopen_ms = end_ms.saturating_sub(span_ms).min(now_ms);
        Blink {
            start_ms: reopen_ms.saturating_sub(1),
            closed_ms: reopen_ms,
            reopen_ms,
            end_ms,
        }
    }

    /// 0 = open, 1 = shut.
    pub fn closure(&self, now_ms: u64) -> f32 {
        let ramp = |from: u64, to: u64| (now_ms - from) as f32 / (to - from).max(1) as f32;
        if now_ms < self.start_ms || now_ms >= self.end_ms {
            0.0
        } else if now_ms < self.closed_ms {
            ramp(self.start_ms, self.closed_ms)
        } else if now_ms < self.reopen_ms {
            1.0
        } else {
            1.0 - ramp(self.reopen_ms, self.end_ms)
        }
    }

    pub fn is_over(&self, now_ms: u64) -> bool {
        now_ms >= self.end_ms
    }
}

/// Milliseconds until a spring in `spring`'s state first covers each of [`PROGRESS_POINTS`] of its
/// distance to `target`, following it forward with the same closed-form step the eyes use. `None` for a
/// negligible move.
fn progress_times(mut spring: Spring, target: Vec2, omega: f32) -> Option<[u64; 4]> {
    let distance = (target - spring.position).length();
    if distance < MIN_MOVE {
        return None;
    }
    let mut times = [None; 4];
    let mut elapsed_s = 0.0;
    while elapsed_s < SIMULATION_HORIZON_S && times[3].is_none() {
        spring.step(target, omega, SIMULATION_STEP_S);
        elapsed_s += SIMULATION_STEP_S;
        let covered = 1.0 - (target - spring.position).length() / distance;
        for (time, progress) in times.iter_mut().zip(PROGRESS_POINTS) {
            if time.is_none() && covered >= progress {
                *time = Some(seconds_to_ms(elapsed_s));
            }
        }
    }
    let horizon_ms = seconds_to_ms(SIMULATION_HORIZON_S);
    Some(times.map(|time| time.unwrap_or(horizon_ms)))
}

/// Fraction of a move a critically damped spring released from rest has covered after `seconds`.
pub fn spring_progress(seconds: f32, omega: f32) -> f32 {
    let phase = omega * seconds;
    1.0 - (1.0 + phase) * expf(-phase)
}

/// Seconds for a critically damped spring released from rest to cover `progress` (0..1) of its move.
pub fn time_to_progress(progress: f32, omega: f32) -> f32 {
    // Progress is monotonic in ωt, so bisect on the phase; 40 halvings of 0..20 is far below 1 ms.
    let (mut low, mut high) = (0.0_f32, 20.0_f32);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if 1.0 - (1.0 + middle) * expf(-middle) < progress {
            low = middle;
        } else {
            high = middle;
        }
    }
    high / omega
}

fn seconds_to_ms(seconds: f32) -> u64 {
    (seconds * 1000.0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_to_progress_inverts_spring_progress() {
        for progress in [0.12, 0.45, 0.88, 0.95] {
            let seconds = time_to_progress(progress, 6.0);
            assert!(
                (spring_progress(seconds, 6.0) - progress).abs() < 1e-4,
                "{progress}"
            );
        }
    }

    #[test]
    fn coupled_blink_timeline_scales_with_spring_stiffness() {
        // Neither stiffness hits the duration clamp, so doubling ω halves every offset.
        let [soft, stiff] = [8.0, 16.0].map(|omega| Blink::during_shift(0, omega));
        let offsets = |blink: Blink| [blink.start_ms, blink.closed_ms, blink.end_ms];
        let ratios = offsets(soft)
            .map(|ms| ms as f32)
            .iter()
            .zip(offsets(stiff))
            .map(|(a, b)| a / b as f32)
            .collect::<alloc::vec::Vec<_>>();
        assert!(
            ratios.iter().all(|ratio| (ratio - 2.0).abs() < 0.03),
            "{soft:?} {stiff:?}"
        );
    }

    #[test]
    fn coupled_blink_lands_on_the_documented_progress_points() {
        let blink = Blink::during_shift(1000, 14.0);
        let progress = |ms: u64| spring_progress((ms - 1000) as f32 / 1000.0, 14.0);
        let points = [
            progress(blink.start_ms),
            progress(blink.closed_ms),
            progress(blink.end_ms),
        ];
        let want = [CLOSE_START_PROGRESS, CLOSED_PROGRESS, OPEN_PROGRESS];
        assert!(
            points
                .iter()
                .zip(want)
                .all(|(got, want)| (got - want).abs() < 0.02),
            "{points:?}"
        );
    }

    #[test]
    fn a_lazy_spring_gets_a_clamped_blink_that_still_opens_before_settling() {
        let blink = Blink::during_shift(0, 6.0);
        let settled_ms = seconds_to_ms(time_to_progress(SETTLED_PROGRESS, 6.0));
        let duration = blink.end_ms - blink.start_ms;
        assert!(
            duration == BLINK_DURATION_MS.1 && blink.end_ms < settled_ms,
            "{blink:?}"
        );
    }

    #[test]
    fn retiming_keeps_the_lid_where_it_is_and_ends_by_the_target() {
        let idle = Blink::during_shift(0, 6.0);
        let target_onset = |now_ms: u64| Blink::during_shift(now_ms, 14.0);
        // Before the idle blink starts, while closing, at the shut instant, and while reopening.
        for now_ms in [50, 150, idle.closed_ms, 400] {
            let retimed = idle.retimed_to(now_ms, target_onset(now_ms));
            let continuous = (retimed.closure(now_ms) - idle.closure(now_ms)).abs() < 0.02;
            let ends_in_time = retimed.end_ms <= target_onset(now_ms).end_ms;
            assert!(continuous && ends_in_time, "at {now_ms}: {retimed:?}");
        }
    }

    #[test]
    fn a_move_from_rest_matches_the_reference_schedule() {
        let springs = [Spring::default(); 2];
        let targets = [Vec2::new(0.6, 0.0); 2];
        let simulated = Blink::during_move(0, springs, targets, 14.0);
        let reference = Blink::during_shift(0, 14.0);
        let offsets = |blink: Blink| [blink.start_ms, blink.closed_ms, blink.end_ms];
        let close = offsets(simulated)
            .iter()
            .zip(offsets(reference))
            .all(|(a, b)| a.abs_diff(b) <= 3);
        assert!(close, "{simulated:?} vs {reference:?}");
    }

    #[test]
    fn an_eye_already_moving_towards_its_target_gets_an_earlier_blink() {
        let moving = Spring {
            position: Vec2::new(0.2, 0.0),
            velocity: Vec2::new(2.0, 0.0),
        };
        let target = [Vec2::new(0.6, 0.0); 2];
        let from_rest = Blink::during_move(0, [Spring::at(Vec2::new(0.2, 0.0)); 2], target, 14.0);
        let already_moving = Blink::during_move(0, [moving; 2], target, 14.0);
        assert!(
            already_moving.end_ms < from_rest.end_ms,
            "{already_moving:?} vs {from_rest:?}"
        );
    }

    #[test]
    fn the_lid_reopens_before_landing_even_when_the_duration_clamp_would_hold_it() {
        // Landing 60 ms after onset is shorter than the 150 ms minimum blink.
        let blink = Blink::at_progress_times(0, [5, 20, 50, 60]);
        assert!(blink.end_ms < 60, "{blink:?}");
    }

    #[test]
    fn closure_follows_the_timeline() {
        let blink = Blink {
            start_ms: 100,
            closed_ms: 200,
            reopen_ms: 250,
            end_ms: 350,
        };
        let cases = [
            (0, 0.0),
            (100, 0.0),
            (150, 0.5),
            (200, 1.0),
            (249, 1.0),
            (300, 0.5),
            (350, 0.0),
        ];
        for (now_ms, want) in cases {
            assert!(
                (blink.closure(now_ms) - want).abs() < 1e-6,
                "{now_ms}: {}",
                blink.closure(now_ms)
            );
        }
    }

    #[test]
    fn routine_blink_lasts_240_ms() {
        let blink = Blink::routine(1000);
        assert_eq!((blink.closure(1100), blink.end_ms), (1.0, 1240));
    }
}

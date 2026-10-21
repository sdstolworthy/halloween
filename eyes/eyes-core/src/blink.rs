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

/// Fractions of the move at which the coupled blink starts closing, is shut, and is open again.
pub const CLOSE_START_PROGRESS: f32 = 0.12;
pub const CLOSED_PROGRESS: f32 = 0.48;
pub const OPEN_PROGRESS: f32 = 0.88;
/// The gaze is considered to have landed once this much of the move is covered.
pub const SETTLED_PROGRESS: f32 = 0.95;
pub const BLINK_DURATION_MS: (u64, u64) = (150, 450);

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

    /// A blink coupled to a gaze shift that starts at `onset_ms` on a spring of stiffness `omega`.
    pub fn during_shift(onset_ms: u64, omega: f32) -> Self {
        let at = |progress: f32| onset_ms + seconds_to_ms(time_to_progress(progress, omega));
        let start_ms = at(CLOSE_START_PROGRESS);
        let duration_ms =
            (at(OPEN_PROGRESS) - start_ms).clamp(BLINK_DURATION_MS.0, BLINK_DURATION_MS.1);
        let end_ms = start_ms + duration_ms;
        let closed_ms = at(CLOSED_PROGRESS)
            .min((start_ms + end_ms) / 2)
            .max(start_ms + 1);
        Self {
            start_ms,
            closed_ms,
            reopen_ms: closed_ms,
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

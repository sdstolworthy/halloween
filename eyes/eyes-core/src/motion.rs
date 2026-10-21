//! Smoothing primitives driven by elapsed time, so motion looks the same at any frame rate.

use libm::expf;

use crate::geometry::Vec2;

/// A critically damped spring: reaches its target as fast as possible without overshooting. Each step uses
/// the closed-form solution, which is exact for any `dt` and therefore stable across frame-rate hiccups.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Spring {
    pub position: Vec2,
    pub velocity: Vec2,
}

impl Spring {
    pub const fn at(position: Vec2) -> Self {
        Self {
            position,
            velocity: Vec2::ZERO,
        }
    }

    /// Advance by `dt_s` towards `target`. `omega` (rad/s) sets the speed: the spring covers ~90% of a
    /// step change in about 3.9 / omega seconds.
    pub fn step(&mut self, target: Vec2, omega: f32, dt_s: f32) {
        let offset = self.position - target;
        let decay = expf(-omega * dt_s);
        let slope = self.velocity + offset * omega;
        self.position = target + (offset + slope * dt_s) * decay;
        self.velocity = (self.velocity - slope * (omega * dt_s)) * decay;
    }
}

/// First-order exponential approach of `current` towards `target` with time constant `tau_s`.
pub fn approach(current: f32, target: f32, tau_s: f32, dt_s: f32) -> f32 {
    current + (target - current) * (1.0 - expf(-dt_s / tau_s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(spring: &mut Spring, target: Vec2, omega: f32, dt_s: f32, steps: usize) -> f32 {
        let mut furthest = spring.position.x;
        for _ in 0..steps {
            spring.step(target, omega, dt_s);
            furthest = furthest.max(spring.position.x);
        }
        furthest
    }

    #[test]
    fn spring_settles_on_its_target() {
        let mut spring = Spring::default();
        run(&mut spring, Vec2::new(1.0, -0.5), 12.0, 1.0 / 30.0, 90);
        assert!((spring.position - Vec2::new(1.0, -0.5)).length() < 1e-3);
    }

    #[test]
    fn spring_never_overshoots() {
        let mut spring = Spring::default();
        let furthest = run(&mut spring, Vec2::new(1.0, 0.0), 12.0, 1.0 / 60.0, 300);
        assert!(furthest <= 1.0, "overshot to {furthest}");
    }

    #[test]
    fn spring_is_frame_rate_independent() {
        let target = Vec2::new(1.0, 0.0);
        let mut fast = Spring::default();
        let mut slow = Spring::default();
        run(&mut fast, target, 10.0, 1.0 / 60.0, 12);
        run(&mut slow, target, 10.0, 1.0 / 15.0, 3);
        assert!((fast.position - slow.position).length() < 1e-4);
    }

    #[test]
    fn spring_survives_a_long_stall() {
        let mut spring = Spring::default();
        spring.step(Vec2::new(0.5, 0.5), 12.0, 30.0);
        assert!((spring.position - Vec2::new(0.5, 0.5)).length() < 1e-6);
    }

    #[test]
    fn approach_covers_63_percent_in_one_time_constant() {
        assert!((approach(0.0, 1.0, 0.5, 0.5) - 0.632_12).abs() < 1e-4);
    }
}

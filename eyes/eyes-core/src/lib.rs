//! Hardware-free logic for the tracking eyes: geometry, configuration, the gaze/blink/idle state machine
//! and the MQTT payload contract. Time is passed in as milliseconds and randomness comes from an injected
//! [`Rng`], so the same inputs always produce the same poses on the host, in the browser and on the device.

#![no_std]

extern crate alloc;

pub mod config;
pub mod eyes;
pub mod geometry;
pub mod layout;
pub mod motion;
pub mod payload;
pub mod rng;
pub mod topics;

pub use config::{ConfigError, EyesConfig, Param};
pub use eyes::{Eyes, Mode, Pose};
pub use geometry::{GazeAngles, Vec2, Vec3};
pub use payload::{parse_distance, PayloadError, Reading};
pub use rng::{Rng, SplitMix64};

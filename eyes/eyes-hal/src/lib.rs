//! The seams between the eyes and the hardware, plus the board-agnostic frame loop that runs on them.
//!
//! The firmware supplies a [`Matrix`] (the HUB75 panel), a [`Clock`] and an [`Inbox`] (MQTT messages);
//! [`App`] does everything else. Because the loop only sees these traits, it runs on the host against
//! the [`mock`] implementations, which is how its message handling is tested.
//!
//! The matrix trait and the double-to-single-buffer ladder are adapted from `flight-hal` in
//! sdstolworthy/flight-rs (rev 2ebde02a4941749724e1132f0514557120769526). That crate is not depended on
//! directly: the repo is private (this one is public, so a git dependency would not build for anyone
//! else) and flight-hal carries flight-panel storage keys, Wi-Fi portal and flight-core coupling the eyes
//! do not need.

#![no_std]

extern crate alloc;

mod app;
pub mod mock;

pub use app::{App, MAX_MESSAGES_PER_FRAME};

use alloc::string::String;
use alloc::vec::Vec;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::DrawTarget;

/// The LED panel as a draw surface.
///
/// Drawing goes to a back buffer when the board could allocate one; [`Matrix::flush`] presents it. A
/// board that fell back to a single buffer draws straight to the live image and `flush` does nothing,
/// which is fine for the eyes because every frame repaints every pixel.
pub trait Matrix: DrawTarget<Color = Rgb888> {
    fn flush(&mut self) -> Result<(), Self::Error>;
    /// 0 is off, 255 full scale. Takes effect no later than the next flush.
    fn set_brightness(&mut self, level: u8);
}

/// Milliseconds from a monotonic clock, independent of any network time.
pub trait Clock {
    fn uptime_ms(&self) -> u64;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub topic: String,
    pub payload: Vec<u8>,
}

/// Messages received from the broker, drained without blocking once per frame.
pub trait Inbox {
    fn poll(&mut self) -> Option<Message>;
}

/// One HUB75 driver configuration to attempt, best first in [`MATRIX_LADDER`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixConfig {
    pub double_buffer: bool,
}

/// A 128x64 double buffer needs two DMA framebuffers of internal SRAM, which a PSRAM-less WROOM-32 may
/// not have once fragmented, so the firmware falls back to a single buffer.
pub const MATRIX_LADDER: [MatrixConfig; 2] = [
    MatrixConfig {
        double_buffer: true,
    },
    MatrixConfig {
        double_buffer: false,
    },
];

/// The first config in `ladder` that `try_init` accepts, trying them in order.
pub fn select_matrix_config(
    ladder: &[MatrixConfig],
    mut try_init: impl FnMut(MatrixConfig) -> bool,
) -> Option<MatrixConfig> {
    ladder.iter().copied().find(|config| try_init(*config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_picks_the_first_config_that_initialises() {
        let double = MatrixConfig {
            double_buffer: true,
        };
        let single = MatrixConfig {
            double_buffer: false,
        };
        let cases: [(&[bool], Option<MatrixConfig>, usize); 3] = [
            (&[true], Some(double), 1),
            (&[false, true], Some(single), 2),
            (&[false, false], None, 2),
        ];
        for (outcomes, want, want_attempts) in cases {
            let mut attempts = 0;
            let got = select_matrix_config(&MATRIX_LADDER, |_| {
                attempts += 1;
                outcomes[attempts - 1]
            });
            assert_eq!((got, attempts), (want, want_attempts), "{outcomes:?}");
        }
    }
}

//! [`eyes_hal::Matrix`] over the vendored HUB75 shim (`components/matrix_shim`), adapted from flight-rs's
//! `firmware/src/matrix.rs` (rev 2ebde02). The I2S/DMA driver refreshes the panel by itself in the
//! background; we only write pixels and flip buffers.
//!
//! `new` walks [`MATRIX_LADDER`]: a double buffer gives tear-free flips, and when its ~2 x 64 KiB of
//! DMA-capable RAM is not available it falls back to a single buffer with a loud warning. The eyes repaint
//! every pixel each frame, so the fallback costs only a brief tear while a frame is drawn.

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use eyes_hal::{select_matrix_config, Matrix, MatrixConfig, MATRIX_LADDER};
use log::{info, warn};

/// HUB75 ribbon to GPIO mapping. `repr(C)`: passed by pointer to the shim's `hub75_pins_t`, which mirrors
/// this field order exactly.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Hub75Pins {
    pub r1: i32,
    pub g1: i32,
    pub b1: i32,
    pub r2: i32,
    pub g2: i32,
    pub b2: i32,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub e: i32,
    pub clk: i32,
    pub lat: i32,
    pub oe: i32,
    pub clkphase: i32,
}

mod ffi {
    use super::Hub75Pins;

    extern "C" {
        /// Returns 0 if the DMA buffers could not be allocated; safe to retry with a cheaper config.
        pub fn hub75_init(
            width: i32,
            height: i32,
            pins: *const Hub75Pins,
            double_buff: bool,
        ) -> i32;
        pub fn hub75_draw_pixel(x: i32, y: i32, r: u8, g: u8, b: u8);
        /// Double buffer: present the back buffer without clearing the new one. Single buffer: no-op.
        pub fn hub75_flip();
        pub fn hub75_set_brightness(level: u8);
    }
}

pub struct Hub75Matrix {
    size: Size,
    config: MatrixConfig,
}

impl Hub75Matrix {
    pub fn new(width: u32, height: u32, pins: Hub75Pins) -> anyhow::Result<Self> {
        let config = select_matrix_config(&MATRIX_LADDER, |config| {
            info!(
                "HUB75 init {width}x{height} double_buffer={} (largest free DMA block {} B)",
                config.double_buffer,
                largest_free_dma_block()
            );
            // SAFETY: `pins` outlives the call and the shim copies it.
            unsafe {
                ffi::hub75_init(width as i32, height as i32, &pins, config.double_buffer) != 0
            }
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "HUB75 DMA init failed for {width}x{height}; largest free DMA block is {} B",
                largest_free_dma_block()
            )
        })?;
        if config.double_buffer {
            info!("HUB75 up, double-buffered");
        } else {
            warn!("HUB75 up SINGLE-buffered: the double buffer did not fit in DMA RAM; frames may briefly tear");
        }
        Ok(Self {
            size: Size::new(width, height),
            config,
        })
    }
}

/// The number that decides whether a framebuffer allocation can succeed: total free heap can look fine
/// while no single block is big enough.
pub fn largest_free_dma_block() -> usize {
    // SAFETY: a pure heap query.
    unsafe { esp_idf_svc::sys::heap_caps_get_largest_free_block(esp_idf_svc::sys::MALLOC_CAP_DMA) }
}

impl OriginDimensions for Hub75Matrix {
    fn size(&self) -> Size {
        self.size
    }
}

impl DrawTarget for Hub75Matrix {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb888>>,
    {
        for Pixel(point, colour) in pixels {
            // SAFETY: the library clips out-of-range coordinates.
            unsafe { ffi::hub75_draw_pixel(point.x, point.y, colour.r(), colour.g(), colour.b()) };
        }
        Ok(())
    }
}

impl Matrix for Hub75Matrix {
    fn flush(&mut self) -> Result<(), Self::Error> {
        // No clear after the flip: the renderer writes every pixel of every frame, so clearing would only
        // cost a full extra pass over the DMA buffer.
        if self.config.double_buffer {
            // SAFETY: no arguments; flips the DMA buffers.
            unsafe { ffi::hub75_flip() };
        }
        Ok(())
    }

    fn set_brightness(&mut self, level: u8) {
        // SAFETY: scalar argument, applied on the next refresh.
        unsafe { ffi::hub75_set_brightness(level) };
    }
}

use core::convert::Infallible;

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use eyes_core::layout::{PANEL_HEIGHT, PANEL_WIDTH};

const WIDTH: usize = PANEL_WIDTH as usize;
const HEIGHT: usize = PANEL_HEIGHT as usize;

/// An in-memory 128x64 panel, for the emulator and for tests. Out-of-bounds pixels are clipped.
#[derive(Clone)]
pub struct Framebuffer {
    pixels: [Rgb888; WIDTH * HEIGHT],
}

impl Framebuffer {
    pub const fn new() -> Self {
        Self {
            pixels: [Rgb888::BLACK; WIDTH * HEIGHT],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Rgb888 {
        self.pixels[y as usize * WIDTH + x as usize]
    }

    /// Row-major pixels.
    pub fn pixels(&self) -> &[Rgb888] {
        &self.pixels
    }

    /// Row-major RGBA8 with opaque alpha, the layout a browser `ImageData` expects.
    pub fn write_rgba(&self, out: &mut [u8; WIDTH * HEIGHT * 4]) {
        for (rgba, pixel) in out.chunks_exact_mut(4).zip(self.pixels.iter()) {
            rgba.copy_from_slice(&[pixel.r(), pixel.g(), pixel.b(), 0xFF]);
        }
    }
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(PANEL_WIDTH, PANEL_HEIGHT)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Rgb888>>,
    {
        for Pixel(point, colour) in pixels {
            if let (Ok(x @ 0..WIDTH), Ok(y @ 0..HEIGHT)) =
                (usize::try_from(point.x), usize::try_from(point.y))
            {
                self.pixels[y * WIDTH + x] = colour;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_out_of_bounds_pixels() {
        let mut framebuffer = Framebuffer::new();
        let outside = [(-1, 0), (0, -1), (128, 0), (0, 64)];
        framebuffer
            .draw_iter(outside.map(|(x, y)| Pixel(Point::new(x, y), Rgb888::WHITE)))
            .unwrap();
        assert!(framebuffer
            .pixels()
            .iter()
            .all(|&pixel| pixel == Rgb888::BLACK));
    }

    #[test]
    fn rgba_is_row_major_and_opaque() {
        let mut framebuffer = Framebuffer::new();
        Pixel(Point::new(1, 2), Rgb888::new(10, 20, 30))
            .draw(&mut framebuffer)
            .unwrap();
        let mut rgba = [0u8; WIDTH * HEIGHT * 4];
        framebuffer.write_rgba(&mut rgba);
        let offset = (2 * WIDTH + 1) * 4;
        assert_eq!(rgba[offset..offset + 4], [10, 20, 30, 255]);
    }
}

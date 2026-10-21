//! Writes a contact sheet of a scripted walk-up, rendered by the real state machine and renderer, to
//! `eyes-emulator/out/contact-sheet.png` (or the path given as the first argument).

mod png;

use std::path::PathBuf;

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use eyes_core::layout::{PANEL_HEIGHT, PANEL_WIDTH};
use eyes_core::{Eyes, EyesConfig, Pose, Reading, SplitMix64};
use eyes_render::Framebuffer;

/// Each LED becomes a `LED_PX` square with a one-pixel dark gap, like the real panel up close.
const LED_PX: u32 = 4;
const COLUMNS: u32 = 2;
const LABEL_PX: u32 = 14;
const TILE_W: u32 = PANEL_WIDTH * LED_PX;
const TILE_H: u32 = PANEL_HEIGHT * LED_PX + LABEL_PX;
const FRAME_MS: u64 = 33;

/// Snapshot times (ms) and what the sensor reports at each moment of the script: idle for 3 s, then a
/// visitor walks from 4 m to 0.3 m over 8 s, lingers, and leaves.
const SNAPSHOTS_MS: [u64; 10] = [
    1_000, 3_000, 4_000, 6_000, 8_000, 10_000, 11_000, 13_000, 15_500, 17_500,
];

fn reading_at(now_ms: u64) -> Reading {
    match now_ms {
        0..3_000 => Reading::Nobody,
        3_000..11_000 => Reading::Distance(4.0 - 3.7 * (now_ms - 3_000) as f32 / 8_000.0),
        11_000..14_000 => Reading::Distance(0.3),
        _ => Reading::Nobody,
    }
}

fn describe(reading: Reading) -> String {
    match reading {
        Reading::Distance(metres) => format!("{metres:.2} m"),
        Reading::Nobody => "nobody".into(),
    }
}

fn scenario() -> Vec<(String, Pose)> {
    let mut eyes = Eyes::new(EyesConfig::DEFAULT, SplitMix64::new(1), 0);
    let mut snapshots = Vec::new();
    let mut now_ms = 0;
    for snapshot_ms in SNAPSHOTS_MS {
        let mut pose = Pose::RESTING;
        while now_ms < snapshot_ms {
            now_ms = (now_ms + FRAME_MS).min(snapshot_ms);
            if now_ms % 100 < FRAME_MS {
                eyes.observe(reading_at(now_ms), now_ms);
            }
            pose = eyes.tick(now_ms);
        }
        let label = format!(
            "t={:.1}s  {}  {:?}",
            now_ms as f32 / 1000.0,
            describe(reading_at(now_ms)),
            eyes.mode()
        );
        snapshots.push((label, pose));
    }
    snapshots
}

struct Sheet {
    width: u32,
    height: u32,
    pixels: Vec<Rgb888>,
}

impl Sheet {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![Rgb888::new(12, 12, 16); (width * height) as usize],
        }
    }

    fn paste(&mut self, panel: &Framebuffer, origin: Point) {
        let dot = Size::new(LED_PX - 1, LED_PX - 1);
        for y in 0..PANEL_HEIGHT {
            for x in 0..PANEL_WIDTH {
                let top_left = origin + Point::new((x * LED_PX) as i32, (y * LED_PX) as i32);
                let _ = self.fill_solid(
                    &embedded_graphics::primitives::Rectangle::new(top_left, dot),
                    panel.pixel(x, y),
                );
            }
        }
    }

    fn rgb(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()])
            .collect()
    }
}

impl OriginDimensions for Sheet {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for Sheet {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb888>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(point, colour) in pixels {
            if (0..self.width as i32).contains(&point.x)
                && (0..self.height as i32).contains(&point.y)
            {
                self.pixels[(point.y as u32 * self.width + point.x as u32) as usize] = colour;
            }
        }
        Ok(())
    }
}

fn main() -> std::io::Result<()> {
    let snapshots = scenario();
    let rows = (snapshots.len() as u32).div_ceil(COLUMNS);
    let mut sheet = Sheet::new(COLUMNS * TILE_W, rows * TILE_H);
    let label_style = MonoTextStyle::new(&FONT_6X10, Rgb888::new(200, 200, 200));
    for (index, (label, pose)) in snapshots.iter().enumerate() {
        let index = index as u32;
        let origin = Point::new(
            ((index % COLUMNS) * TILE_W) as i32,
            ((index / COLUMNS) * TILE_H) as i32,
        );
        let mut panel = Framebuffer::new();
        let Ok(()) = eyes_render::draw(pose, &mut panel);
        sheet.paste(&panel, origin + Point::new(0, LABEL_PX as i32));
        let Ok(_) = Text::new(label, origin + Point::new(4, 10), label_style).draw(&mut sheet);
    }

    let path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out/contact-sheet.png"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        png::encode_rgb(sheet.width, sheet.height, &sheet.rgb()),
    )?;
    println!("wrote {}", path.display());
    Ok(())
}

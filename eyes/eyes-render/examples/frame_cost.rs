//! Host micro-benchmark of one full-panel render, to compare shader changes. Absolute numbers say little
//! about the ESP32 (no FFI pixel writes here); the ratio between runs is what matters.
//! Run: cargo run --release -p eyes-render --example frame_cost

use std::hint::black_box;
use std::time::Instant;

use eyes_core::{Pose, Vec2};
use eyes_render::{draw, Framebuffer};

const FRAMES: u32 = 5_000;

fn main() {
    let poses = [
        Pose::RESTING,
        Pose {
            gaze: [Vec2::new(0.6, -0.2), Vec2::new(0.4, -0.2)],
            squint: 0.3,
            dilation: 0.9,
            ..Pose::RESTING
        },
        Pose {
            blink: 0.5,
            ..Pose::RESTING
        },
    ];
    let mut framebuffer = Framebuffer::new();
    let started = Instant::now();
    for frame in 0..FRAMES {
        let pose = &poses[frame as usize % poses.len()];
        let Ok(()) = draw(black_box(pose), &mut framebuffer);
        black_box(&framebuffer);
    }
    let per_frame = started.elapsed() / FRAMES;
    println!(
        "{FRAMES} frames, {:.1} µs per 128x64 frame",
        per_frame.as_secs_f64() * 1e6
    );
}

//! Draws a [`Pose`] onto any 128x64 `embedded-graphics` target. The same code runs in the browser
//! emulator and on the device, so the emulator shows exactly the pixels the panel will.
//!
//! Every pixel is shaded analytically (sclera, iris, pupil, lids) and written in one `fill_contiguous`
//! pass, so a frame is always a complete repaint: no clear is needed and a single-buffered panel never
//! shows a half-erased eye.

#![no_std]

mod framebuffer;

pub use framebuffer::Framebuffer;

use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use eyes_core::layout::{EYE_CENTRES_PX, PANEL_HEIGHT, PANEL_WIDTH};
use eyes_core::{Pose, Vec2};
use libm::sqrtf;

/// Sclera half-width and half-height in pixels.
const SCLERA: Vec2 = Vec2::new(27.0, 20.0);
/// Shut lids meet this far below the eye centre, so the upper lid does most of the travel like a real one.
const LID_MEET_PX: f32 = 6.0;
/// How far a full squint drops the upper lid at the inner corner (and lifts it at the outer), for a
/// suspicious frown.
const SQUINT_TILT_PX: f32 = 12.0;
const LID_SHADOW_PX: f32 = 3.0;
const IRIS_RADIUS_PX: f32 = 12.0;
const PUPIL_RADIUS_PX: (f32, f32) = (3.5, 8.0);
/// Pupil centre travel at the edge of the gaze disc. The iris may slide partly under the lids, which reads
/// as looking up or down rather than as clipping.
const TRAVEL_PX: Vec2 = Vec2::new(
    SCLERA.x - IRIS_RADIUS_PX * 0.75,
    SCLERA.y - IRIS_RADIUS_PX * 0.6,
);
const GLINT_OFFSET: Vec2 = Vec2::new(-0.45, -0.45);
const GLINT_RADIUS_PX: f32 = 1.6;

// Deliberately not white: two white eyeballs would light a large share of the panel at full current.
const SCLERA_COLOUR: [f32; 3] = [190.0, 175.0, 135.0];
const IRIS_INNER: [f32; 3] = [255.0, 170.0, 20.0];
const IRIS_OUTER: [f32; 3] = [185.0, 30.0, 0.0];
const LIMBUS: Rgb888 = Rgb888::new(90, 10, 0);
const GLINT: Rgb888 = Rgb888::new(255, 240, 220);

/// Render `pose` over the whole panel area of `target`.
pub fn draw<D>(pose: &Pose, target: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget,
    D::Color: From<Rgb888>,
{
    let eyes = [Eye::new(pose, 0), Eye::new(pose, 1)];
    let area = Rectangle::new(Point::zero(), Size::new(PANEL_WIDTH, PANEL_HEIGHT));
    let colours = (0..PANEL_HEIGHT).flat_map(move |y| {
        (0..PANEL_WIDTH).map(move |x| {
            let centre = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let eye = &eyes[usize::from(x >= PANEL_WIDTH / 2)];
            D::Color::from(eye.shade(centre))
        })
    });
    target.fill_contiguous(&area, colours)
}

#[derive(Clone, Copy)]
/// One eye's per-frame parameters, resolved once so the per-pixel work is plain arithmetic.
struct Eye {
    centre: Vec2,
    iris: Vec2,
    pupil_radius: f32,
    /// 0 = open, 1 = shut.
    closure: f32,
    /// Lid tilt in pixels per unit of horizontal position, signed so the inner corner drops.
    tilt: f32,
}

impl Eye {
    fn new(pose: &Pose, index: usize) -> Self {
        let (x, y) = EYE_CENTRES_PX[index];
        let centre = Vec2::new(x, y);
        let gaze = sanitise(pose.gaze[index]).clamped_to_unit_disc();
        let squint = unit(pose.squint);
        // Screen-left eye's nose side is +x, the other eye's is -x.
        let towards_nose = if index == 0 { 1.0 } else { -1.0 };
        Self {
            centre,
            // Screen y grows downwards while gaze y is up.
            iris: centre + Vec2::new(gaze.x * TRAVEL_PX.x, -gaze.y * TRAVEL_PX.y),
            pupil_radius: lerp(PUPIL_RADIUS_PX.0, PUPIL_RADIUS_PX.1, unit(pose.dilation)),
            closure: unit(pose.blink).max(squint),
            tilt: towards_nose * squint * SQUINT_TILT_PX,
        }
    }

    fn shade(&self, pixel: Vec2) -> Rgb888 {
        let offset = pixel - self.centre;
        let across = offset.x / SCLERA.x;
        if across.abs() >= 1.0 {
            return Rgb888::BLACK;
        }
        let height = sqrtf(1.0 - across * across);
        // The lids follow the sclera's elliptical outline when open and converge on the meeting line as
        // they close; with `closure` 0 the visible region is exactly the ellipse.
        let opening = 1.0 - self.closure;
        let meet = LID_MEET_PX * self.closure;
        let upper_lid = meet - (SCLERA.y + meet) * opening * height + self.tilt * across;
        let lower_lid = meet + (SCLERA.y - meet) * opening * height;
        if offset.y <= upper_lid || offset.y >= lower_lid {
            return Rgb888::BLACK;
        }

        let from_iris = pixel - self.iris;
        let iris_distance = from_iris.length();
        if iris_distance < self.pupil_radius {
            let glint = from_iris - GLINT_OFFSET * self.pupil_radius;
            return if glint.length() < GLINT_RADIUS_PX {
                GLINT
            } else {
                Rgb888::BLACK
            };
        }
        let colour = if iris_distance >= IRIS_RADIUS_PX {
            // Darken towards the rim so the eyeball reads as a sphere.
            let rim = across * across + (offset.y / SCLERA.y) * (offset.y / SCLERA.y);
            scale(SCLERA_COLOUR, 1.0 - 0.6 * rim.min(1.0))
        } else if iris_distance >= IRIS_RADIUS_PX - 1.2 {
            LIMBUS
        } else {
            let t = (iris_distance - self.pupil_radius) / (IRIS_RADIUS_PX - self.pupil_radius);
            mix(IRIS_INNER, IRIS_OUTER, t)
        };
        if offset.y - upper_lid < LID_SHADOW_PX {
            dim(colour, 0.55)
        } else {
            colour
        }
    }
}

/// Poses come from the state machine and are always finite, but a renderer must never panic or smear
/// garbage across the panel, so anything non-finite becomes "centred".
fn sanitise(gaze: Vec2) -> Vec2 {
    if gaze.x.is_finite() && gaze.y.is_finite() {
        gaze
    } else {
        Vec2::ZERO
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn mix(from: [f32; 3], to: [f32; 3], t: f32) -> Rgb888 {
    let channel = |i: usize| lerp(from[i], to[i], t) as u8;
    Rgb888::new(channel(0), channel(1), channel(2))
}

fn scale(colour: [f32; 3], factor: f32) -> Rgb888 {
    let channel = |i: usize| (colour[i] * factor) as u8;
    Rgb888::new(channel(0), channel(1), channel(2))
}

fn dim(colour: Rgb888, factor: f32) -> Rgb888 {
    scale(
        [
            f32::from(colour.r()),
            f32::from(colour.g()),
            f32::from(colour.b()),
        ],
        factor,
    )
}

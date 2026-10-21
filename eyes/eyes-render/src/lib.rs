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
use eyes_core::{Look, Pose, Vec2};
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

// Evil look: black sclera, a red-hot pupil fading through a glowing red iris. Pure black would make the
// eye vanish against the dark panel, so the sclera keeps a faint dark-red rim that brightens towards the
// edge, and the iris blooms a little onto it. Mostly-black eyes draw far less current than the bone-white
// normal ones, so this stays inside the same brightness budget.
const EVIL_PUPIL_CORE: [f32; 3] = [255.0, 120.0, 50.0];
const EVIL_PUPIL_EDGE: [f32; 3] = [255.0, 30.0, 0.0];
const EVIL_IRIS_INNER: [f32; 3] = [140.0, 0.0, 0.0];
const EVIL_IRIS_OUTER: [f32; 3] = [70.0, 0.0, 0.0];
const EVIL_LIMBUS: Rgb888 = Rgb888::new(35, 0, 0);
const EVIL_SCLERA_CENTRE_RED: f32 = 8.0;
const EVIL_SCLERA_RIM_RED: f32 = 80.0;
const EVIL_BLOOM_PX: f32 = 3.5;
const EVIL_BLOOM_RED: f32 = 110.0;
/// The evil brow narrows the eye a little and slants each upper lid down towards the nose. It only ever
/// lowers the lid (never lifts the outer corner above the eyeball), so the eye stays an eye shape.
const EVIL_NARROWING: f32 = 0.22;
const EVIL_TILT_PX: f32 = 16.0;

/// Render `pose` over the whole panel area of `target`.
pub fn draw<D>(pose: &Pose, target: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget,
    D::Color: From<Rgb888>,
{
    let eyes = [Eye::new(pose, 0), Eye::new(pose, 1)];
    let area = Rectangle::new(Point::zero(), Size::new(PANEL_WIDTH, PANEL_HEIGHT));
    let colours = (0..PANEL_HEIGHT).flat_map(move |y| {
        let row_centre = y as f32 + 0.5;
        (0..PANEL_WIDTH).map(move |x| {
            let eye = &eyes[(x / EYE_COLUMNS) as usize];
            D::Color::from(eye.shade((x % EYE_COLUMNS) as usize, row_centre))
        })
    });
    target.fill_contiguous(&area, colours)
}

/// Each eye owns one half of the panel.
const EYE_COLUMNS: u32 = PANEL_WIDTH / 2;

/// Everything about a pixel column that doesn't depend on the row, computed once per frame.
#[derive(Clone, Copy, Default)]
struct Column {
    /// Horizontal position across the sclera, -1..1; outside that the column is background.
    across: f32,
    upper_lid: f32,
    lower_lid: f32,
    /// Pixel-centre x relative to the iris centre.
    from_iris_x: f32,
}

#[derive(Clone, Copy)]
/// One eye's per-frame parameters, resolved once so the per-pixel work is a few multiplies and compares.
struct Eye {
    centre_y: f32,
    iris_y: f32,
    pupil_radius: f32,
    pupil_radius_squared: f32,
    glint: Vec2,
    evil: bool,
    columns: [Column; EYE_COLUMNS as usize],
}

const IRIS_RADIUS_SQUARED: f32 = IRIS_RADIUS_PX * IRIS_RADIUS_PX;
const LIMBUS_INNER_SQUARED: f32 = (IRIS_RADIUS_PX - 1.2) * (IRIS_RADIUS_PX - 1.2);
const GLINT_RADIUS_SQUARED: f32 = GLINT_RADIUS_PX * GLINT_RADIUS_PX;
const EVIL_BLOOM_OUTER_SQUARED: f32 =
    (IRIS_RADIUS_PX + EVIL_BLOOM_PX) * (IRIS_RADIUS_PX + EVIL_BLOOM_PX);

impl Eye {
    fn new(pose: &Pose, index: usize) -> Self {
        let (x, y) = EYE_CENTRES_PX[index];
        let centre = Vec2::new(x, y);
        let gaze = sanitise(pose.gaze[index]).clamped_to_unit_disc();
        let squint = unit(pose.squint);
        // Screen y grows downwards while gaze y is up.
        let iris = centre + Vec2::new(gaze.x * TRAVEL_PX.x, -gaze.y * TRAVEL_PX.y);
        let pupil_radius = lerp(PUPIL_RADIUS_PX.0, PUPIL_RADIUS_PX.1, unit(pose.dilation));
        let brow = unit(pose.brow);
        let closure = unit(pose.blink).max(squint).max(brow * EVIL_NARROWING);
        // Screen-left eye's nose side is +x, the other eye's is -x; the lid drops on the nose side.
        // +1 for the screen-left eye, -1 for the other, as arithmetic rather than a float select (see
        // `Eyes::tick` for the Xtensa backend bug that avoids).
        let towards_nose = 1.0 - 2.0 * index as f32;
        // The squint's tilt lifts the outer corner, so it fades out as the blink closes: a shut lid is
        // shut whatever the squint. Scaled by the blink alone, not the squint's own narrowing, so open
        // squinting eyes keep their shape.
        let tilt = towards_nose * squint * SQUINT_TILT_PX * (1.0 - unit(pose.blink));
        let brow_tilt = towards_nose * brow * EVIL_TILT_PX;
        // The lids follow the sclera's elliptical outline when open and converge on the meeting line as
        // they close; with `closure` 0 the visible region is exactly the ellipse.
        let opening = 1.0 - closure;
        let meet = LID_MEET_PX * closure;
        let first_column = (index as u32 * EYE_COLUMNS) as f32;
        let columns = core::array::from_fn(|column| {
            let pixel_x = first_column + column as f32 + 0.5;
            let across = (pixel_x - centre.x) / SCLERA.x;
            let height = if across.abs() < 1.0 {
                sqrtf(1.0 - across * across)
            } else {
                0.0
            };
            Column {
                across,
                upper_lid: meet - (SCLERA.y + meet) * opening * height
                    + tilt * across
                    + (brow_tilt * across).max(0.0),
                lower_lid: meet + (SCLERA.y - meet) * opening * height,
                from_iris_x: pixel_x - iris.x,
            }
        });
        Self {
            centre_y: centre.y,
            iris_y: iris.y,
            pupil_radius,
            pupil_radius_squared: pupil_radius * pupil_radius,
            glint: GLINT_OFFSET * pupil_radius,
            evil: pose.look == Look::Evil,
            columns,
        }
    }

    fn shade(&self, column: usize, row_centre: f32) -> Rgb888 {
        let column = &self.columns[column];
        let offset_y = row_centre - self.centre_y;
        if column.across.abs() >= 1.0
            || offset_y <= column.upper_lid
            || offset_y >= column.lower_lid
        {
            return Rgb888::BLACK;
        }

        let from_iris = Vec2::new(column.from_iris_x, row_centre - self.iris_y);
        let iris_distance_squared = from_iris.x * from_iris.x + from_iris.y * from_iris.y;
        let colour = if self.evil {
            self.evil_colour(column, offset_y, iris_distance_squared)
        } else {
            match self.normal_colour(column, offset_y, from_iris, iris_distance_squared) {
                Shade::Lit(colour) => colour,
                Shade::Unshadowed(colour) => return colour,
            }
        };
        if offset_y - column.upper_lid < LID_SHADOW_PX {
            dim(colour, 0.55)
        } else {
            colour
        }
    }

    fn normal_colour(
        &self,
        column: &Column,
        offset_y: f32,
        from_iris: Vec2,
        iris_distance_squared: f32,
    ) -> Shade {
        if iris_distance_squared < self.pupil_radius_squared {
            let glint = from_iris - self.glint;
            return Shade::Unshadowed(
                if glint.x * glint.x + glint.y * glint.y < GLINT_RADIUS_SQUARED {
                    GLINT
                } else {
                    Rgb888::BLACK
                },
            );
        }
        Shade::Lit(if iris_distance_squared >= IRIS_RADIUS_SQUARED {
            // Darken towards the rim so the eyeball reads as a sphere.
            let down = offset_y / SCLERA.y;
            let rim = column.across * column.across + down * down;
            scale(SCLERA_COLOUR, 1.0 - 0.6 * rim.min(1.0))
        } else if iris_distance_squared >= LIMBUS_INNER_SQUARED {
            LIMBUS
        } else {
            let t = (sqrtf(iris_distance_squared) - self.pupil_radius)
                / (IRIS_RADIUS_PX - self.pupil_radius);
            mix(IRIS_INNER, IRIS_OUTER, t)
        })
    }

    fn evil_colour(&self, column: &Column, offset_y: f32, iris_distance_squared: f32) -> Rgb888 {
        if iris_distance_squared < self.pupil_radius_squared {
            let t = sqrtf(iris_distance_squared) / self.pupil_radius;
            return mix(EVIL_PUPIL_CORE, EVIL_PUPIL_EDGE, t);
        }
        if iris_distance_squared < LIMBUS_INNER_SQUARED {
            let t = (sqrtf(iris_distance_squared) - self.pupil_radius)
                / (IRIS_RADIUS_PX - self.pupil_radius);
            return mix(EVIL_IRIS_INNER, EVIL_IRIS_OUTER, t);
        }
        if iris_distance_squared < IRIS_RADIUS_SQUARED {
            return EVIL_LIMBUS;
        }
        let down = offset_y / SCLERA.y;
        let rim = (column.across * column.across + down * down).min(1.0);
        let mut red =
            EVIL_SCLERA_CENTRE_RED + (EVIL_SCLERA_RIM_RED - EVIL_SCLERA_CENTRE_RED) * rim * rim;
        if iris_distance_squared < EVIL_BLOOM_OUTER_SQUARED {
            let bloom = 1.0 - (sqrtf(iris_distance_squared) - IRIS_RADIUS_PX) / EVIL_BLOOM_PX;
            red += EVIL_BLOOM_RED * bloom * bloom;
        }
        Rgb888::new(red.min(255.0) as u8, 0, 0)
    }
}

/// A shaded pixel, and whether the lid's shadow may darken it (the pupil's glint and black stay as are).
enum Shade {
    Lit(Rgb888),
    Unshadowed(Rgb888),
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

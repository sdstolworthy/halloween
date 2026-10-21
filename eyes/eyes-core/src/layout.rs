//! Where the eyes sit on the 128x64 panel. Shared by the geometry (to find each eye's physical position)
//! and the renderer (to draw it), so the two can never disagree.

pub const PANEL_WIDTH: u32 = 128;
pub const PANEL_HEIGHT: u32 = 64;

/// Eye centres in pixels, screen-left eye first. Each eye is centred in its half of the panel.
pub const EYE_CENTRES_PX: [(f32, f32); 2] = [(32.0, 32.0), (96.0, 32.0)];

/// Horizontal pixel distance between the two eye centres.
pub const EYE_SPACING_PX: f32 = EYE_CENTRES_PX[1].0 - EYE_CENTRES_PX[0].0;

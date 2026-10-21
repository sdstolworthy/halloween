//! Where each eye has to look.
//!
//! World frame (right-handed, metres): origin on the ground directly below the sensor, +y along the
//! walkway towards the approaching visitor (the sensor's aim axis), +x lateral, +z up. The walkway is
//! narrow, so the visitor's face is taken to be on the sensor axis at `(0, distance, face_height)`.
//!
//! The panel has its own orthonormal frame: `normal` points out of the LEDs towards whoever is watching,
//! `right` is the direction of increasing pixel x as seen by that watcher, and `up` is decreasing pixel
//! y. A gaze angle is the direction from one eye to the face expressed in that frame.

use core::ops::{Add, Mul, Sub};

use libm::{atan2f, cosf, sinf, sqrtf};

use crate::config::EyesConfig;
use crate::layout::{EYE_CENTRES_PX, PANEL_HEIGHT, PANEL_WIDTH};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }
}

/// A 2-D point; as a gaze, `x` is towards screen-right and `y` towards screen-up, both within the unit
/// disc where 1 is the edge of the pupil's travel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        sqrtf(self.x * self.x + self.y * self.y)
    }

    pub fn clamped_to_unit_disc(self) -> Self {
        let length = self.length();
        if length > 1.0 {
            self * (1.0 / length)
        } else {
            self
        }
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }
}

/// Direction from an eye to the face, relative to the panel normal. Positive yaw is towards screen-right,
/// positive pitch towards screen-up. Unclamped: a face behind the panel gives |yaw| > 90.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GazeAngles {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
}

impl GazeAngles {
    /// The target is behind the panel's plane, where an eye can't look.
    pub fn is_behind(&self) -> bool {
        self.yaw_deg.abs() > 90.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelFrame {
    pub centre: Vec3,
    pub normal: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

impl PanelFrame {
    pub fn new(config: &EyesConfig) -> Self {
        let (sin_yaw, cos_yaw) = (
            sinf(config.panel_yaw_deg.to_radians()),
            cosf(config.panel_yaw_deg.to_radians()),
        );
        let (sin_pitch, cos_pitch) = (
            sinf(config.panel_pitch_deg.to_radians()),
            cosf(config.panel_pitch_deg.to_radians()),
        );
        Self {
            centre: Vec3::new(
                config.eyes_offset_x_m,
                config.eyes_offset_y_m,
                config.eyes_height_m,
            ),
            normal: Vec3::new(sin_yaw * cos_pitch, cos_yaw * cos_pitch, sin_pitch),
            // A watcher facing the panel looks along -normal, so their right hand (screen-right) is
            // up x normal. Pitch tilts about this axis, so it doesn't change.
            right: Vec3::new(-cos_yaw, sin_yaw, 0.0),
            up: Vec3::new(-sin_pitch * sin_yaw, -sin_pitch * cos_yaw, cos_pitch),
        }
    }

    /// Physical position of each eye centre, screen-left eye first.
    pub fn eye_positions(&self, pixel_pitch_mm: f32) -> [Vec3; 2] {
        let metres_per_pixel = pixel_pitch_mm / 1000.0;
        let panel_centre_px = (PANEL_WIDTH as f32 / 2.0, PANEL_HEIGHT as f32 / 2.0);
        EYE_CENTRES_PX.map(|(x_px, y_px)| {
            let right = (x_px - panel_centre_px.0) * metres_per_pixel;
            let up = (panel_centre_px.1 - y_px) * metres_per_pixel;
            self.centre + self.right * right + self.up * up
        })
    }

    pub fn angles_to(&self, eye: Vec3, target: Vec3) -> GazeAngles {
        let to_target = target - eye;
        let forward = to_target.dot(self.normal);
        let right = to_target.dot(self.right);
        let up = to_target.dot(self.up);
        GazeAngles {
            yaw_deg: atan2f(right, forward).to_degrees(),
            pitch_deg: atan2f(up, sqrtf(forward * forward + right * right)).to_degrees(),
        }
    }
}

pub fn face_position(config: &EyesConfig, distance_m: f32) -> Vec3 {
    Vec3::new(0.0, distance_m, config.face_height_m)
}

/// Gaze angles for both eyes (screen-left first) when the visitor is `distance_m` from the sensor. Each
/// eye aims from its own position, so the eyes converge as the visitor gets close.
pub fn gaze_angles(config: &EyesConfig, distance_m: f32) -> [GazeAngles; 2] {
    let panel = PanelFrame::new(config);
    let face = face_position(config, distance_m);
    panel
        .eye_positions(config.pixel_pitch_mm)
        .map(|eye| panel.angles_to(eye, face))
}

/// Pupil position for a gaze direction. An eyeball rotating by θ moves its pupil by r·sin θ, so the offset
/// follows sin θ, normalised so `max_gaze_deg` reaches the edge of travel. Angles beyond the maximum,
/// including a target behind the panel, pin the pupil to the edge rather than wrapping around.
pub fn gaze_point(angles: GazeAngles, max_gaze_deg: f32) -> Vec2 {
    let edge = sinf(max_gaze_deg.to_radians());
    let along = |degrees: f32| sinf(degrees.clamp(-max_gaze_deg, max_gaze_deg).to_radians()) / edge;
    Vec2::new(along(angles.yaw_deg), along(angles.pitch_deg)).clamped_to_unit_disc()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Param;
    use core::f32::consts::FRAC_1_SQRT_2;

    const TOLERANCE: f32 = 1e-3;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < TOLERANCE
    }

    fn close3(a: Vec3, b: Vec3) -> bool {
        close(a.x, b.x) && close(a.y, b.y) && close(a.z, b.z)
    }

    fn config(changes: &[(Param, f32)]) -> EyesConfig {
        changes
            .iter()
            .fold(EyesConfig::DEFAULT, |config, &(param, value)| {
                config.with(param, value).unwrap()
            })
    }

    #[test]
    fn panel_axes_follow_yaw_and_pitch() {
        struct Case {
            yaw: f32,
            pitch: f32,
            normal: Vec3,
            right: Vec3,
            up: Vec3,
        }
        let cases = [
            Case {
                yaw: 0.0,
                pitch: 0.0,
                normal: Vec3::new(0.0, 1.0, 0.0),
                right: Vec3::new(-1.0, 0.0, 0.0),
                up: Vec3::new(0.0, 0.0, 1.0),
            },
            Case {
                yaw: 90.0,
                pitch: 0.0,
                normal: Vec3::new(1.0, 0.0, 0.0),
                right: Vec3::new(0.0, 1.0, 0.0),
                up: Vec3::new(0.0, 0.0, 1.0),
            },
            Case {
                yaw: 0.0,
                pitch: 90.0 - 30.0,
                normal: Vec3::new(0.0, 0.5, 0.866),
                right: Vec3::new(-1.0, 0.0, 0.0),
                up: Vec3::new(0.0, -0.866, 0.5),
            },
        ];
        for case in cases {
            let panel = PanelFrame::new(&config(&[
                (Param::PanelYaw, case.yaw),
                (Param::PanelPitch, case.pitch),
            ]));
            let axes = [panel.normal, panel.right, panel.up];
            let want = [case.normal, case.right, case.up];
            assert!(
                axes.iter().zip(want).all(|(&got, want)| close3(got, want)),
                "yaw {} pitch {}: {:?}",
                case.yaw,
                case.pitch,
                axes
            );
        }
    }

    #[test]
    fn eyes_sit_either_side_of_the_panel_centre() {
        let config = config(&[
            (Param::EyesOffsetX, 0.0),
            (Param::EyesOffsetY, 0.0),
            (Param::EyesHeight, 2.0),
        ]);
        let eyes = PanelFrame::new(&config).eye_positions(config.pixel_pitch_mm);
        // Screen-left is world +x for a panel facing down the walkway.
        assert!(
            close3(eyes[0], Vec3::new(0.096, 0.0, 2.0))
                && close3(eyes[1], Vec3::new(-0.096, 0.0, 2.0))
        );
    }

    #[test]
    fn gaze_angles_for_representative_installs() {
        struct Case {
            name: &'static str,
            changes: &'static [(Param, f32)],
            distance: f32,
            /// Expected (yaw, pitch) per eye, screen-left eye first.
            want: [(f32, f32); 2],
        }
        let on_axis_at_face_height: &[(Param, f32)] = &[
            (Param::EyesOffsetX, 0.0),
            (Param::EyesOffsetY, 0.0),
            (Param::EyesHeight, 1.6),
        ];
        let cases = [
            Case {
                name: "on axis at face height converges slightly at 2 m",
                changes: on_axis_at_face_height,
                distance: 2.0,
                // atan(0.096 / 2) = 2.748 degrees, each eye turning towards the nose.
                want: [(2.748, 0.0), (-2.748, 0.0)],
            },
            Case {
                name: "on axis converges strongly up close",
                changes: on_axis_at_face_height,
                distance: 0.3,
                want: [(17.745, 0.0), (-17.745, 0.0)],
            },
            Case {
                name: "panel 1 m to the visitor's left looks towards screen-right",
                changes: &[
                    (Param::EyesOffsetX, 1.0),
                    (Param::EyesOffsetY, 0.0),
                    (Param::EyesHeight, 1.6),
                ],
                distance: 1.0,
                // atan(1.096 / 1) and atan(0.904 / 1).
                want: [(47.622, 0.0), (42.114, 0.0)],
            },
            Case {
                name: "panel above the face looks down",
                changes: &[
                    (Param::EyesOffsetX, 0.0),
                    (Param::EyesOffsetY, 0.0),
                    (Param::EyesHeight, 2.6),
                ],
                distance: 1.0,
                // Eye forward 1 m, lateral 0.096 m, 1 m below: atan2(-1, hypot(1, 0.096)).
                want: [(5.484, -44.869), (-5.484, -44.869)],
            },
            Case {
                name: "panel turned to face across the walkway",
                changes: &[
                    (Param::EyesOffsetX, -1.0),
                    (Param::EyesOffsetY, 1.0),
                    (Param::EyesHeight, 1.6),
                    (Param::PanelYaw, 90.0),
                ],
                distance: 1.0,
                // Face is straight in front; eyes are 0.096 m either side along y.
                want: [(5.484, 0.0), (-5.484, 0.0)],
            },
            Case {
                name: "face behind the panel gives angles past 90 degrees",
                changes: &[
                    (Param::EyesOffsetX, 0.0),
                    (Param::EyesOffsetY, 2.0),
                    (Param::EyesHeight, 1.6),
                ],
                distance: 1.0,
                want: [(174.516, 0.0), (-174.516, 0.0)],
            },
        ];
        for case in cases {
            let angles = gaze_angles(&config(case.changes), case.distance);
            let got = angles.map(|a| (a.yaw_deg, a.pitch_deg));
            let matches =
                got.iter()
                    .zip(case.want)
                    .all(|(&(yaw, pitch), (want_yaw, want_pitch))| {
                        close(yaw, want_yaw) && close(pitch, want_pitch)
                    });
            assert!(matches, "{}: got {:?}", case.name, got);
        }
    }

    #[test]
    fn gaze_point_maps_angles_into_the_unit_disc() {
        let cases = [
            ((0.0, 0.0), Vec2::new(0.0, 0.0)),
            ((60.0, 0.0), Vec2::new(1.0, 0.0)),
            ((-60.0, 0.0), Vec2::new(-1.0, 0.0)),
            ((0.0, 30.0), Vec2::new(0.0, 0.57735)),
            ((90.0, 0.0), Vec2::new(1.0, 0.0)),
            ((175.0, 0.0), Vec2::new(1.0, 0.0)),
            ((60.0, -60.0), Vec2::new(FRAC_1_SQRT_2, -FRAC_1_SQRT_2)),
        ];
        for ((yaw_deg, pitch_deg), want) in cases {
            let got = gaze_point(GazeAngles { yaw_deg, pitch_deg }, 60.0);
            assert!(
                close(got.x, want.x) && close(got.y, want.y),
                "({yaw_deg}, {pitch_deg}) -> {got:?}"
            );
        }
    }
}

use embedded_graphics::pixelcolor::{Rgb565, Rgb888};
use embedded_graphics::prelude::*;
use eyes_core::geometry::{gaze_angles, gaze_point};
use eyes_core::{EyesConfig, Look, Pose, Vec2};
use eyes_render::{draw, Framebuffer};

fn render(pose: &Pose) -> Framebuffer {
    let mut framebuffer = Framebuffer::new();
    draw(pose, &mut framebuffer).unwrap();
    framebuffer
}

fn looking(gaze: Vec2) -> Pose {
    Pose {
        gaze: [gaze; 2],
        ..Pose::RESTING
    }
}

fn is_iris(colour: Rgb888) -> bool {
    colour.r() > 120 && u16::from(colour.r()) > u16::from(colour.g()) + 60
}

/// Mean position of the iris pixels in one eye's half of the panel (0 = screen-left).
fn iris_centroid(framebuffer: &Framebuffer, eye: u32) -> (f32, f32) {
    let points: Vec<(u32, u32)> = (0..64)
        .flat_map(|y| (eye * 64..eye * 64 + 64).map(move |x| (x, y)))
        .filter(|&(x, y)| is_iris(framebuffer.pixel(x, y)))
        .collect();
    let count = points.len() as f32;
    let sum = points.iter().fold((0.0, 0.0), |(sx, sy), &(x, y)| {
        (sx + x as f32, sy + y as f32)
    });
    (sum.0 / count, sum.1 / count)
}

fn lit(framebuffer: &Framebuffer) -> usize {
    framebuffer
        .pixels()
        .iter()
        .filter(|&&pixel| pixel != Rgb888::BLACK)
        .count()
}

fn fnv1a(framebuffer: &Framebuffer) -> u32 {
    framebuffer
        .pixels()
        .iter()
        .flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()])
        .fold(0x811C_9DC5, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        })
}

#[test]
fn resting_eyes_have_pupil_iris_and_sclera_in_place() {
    let framebuffer = render(&Pose::RESTING);
    let samples = [
        (32, 32, "pupil", framebuffer.pixel(32, 32) == Rgb888::BLACK),
        (40, 32, "iris", is_iris(framebuffer.pixel(40, 32))),
        (50, 32, "sclera", {
            let colour = framebuffer.pixel(50, 32);
            colour.r() > 80 && colour.g() > 80 && colour.b() > 50
        }),
        (0, 0, "background", framebuffer.pixel(0, 0) == Rgb888::BLACK),
        (
            64,
            32,
            "gap between the eyes",
            framebuffer.pixel(64, 32) == Rgb888::BLACK,
        ),
    ];
    for (x, y, what, ok) in samples {
        assert!(ok, "{what} at ({x}, {y}) is {:?}", framebuffer.pixel(x, y));
    }
}

#[test]
fn eyes_are_mirror_images_when_looking_straight_ahead() {
    let framebuffer = render(&Pose::RESTING);
    let mirrored =
        (0..64).all(|y| (0..64).all(|x| framebuffer.pixel(x, y) == framebuffer.pixel(x + 64, y)));
    assert!(mirrored, "both eyes should be identical copies at rest");
}

#[test]
fn iris_follows_the_gaze() {
    let (rest_x, rest_y) = iris_centroid(&render(&Pose::RESTING), 0);
    let cases = [
        ("right", Vec2::new(1.0, 0.0), (10.0, 0.0)),
        ("left", Vec2::new(-1.0, 0.0), (-10.0, 0.0)),
        ("up", Vec2::new(0.0, 1.0), (0.0, -4.0)),
        ("down", Vec2::new(0.0, -1.0), (0.0, 4.0)),
    ];
    for (name, gaze, (min_dx, min_dy)) in cases {
        let (x, y) = iris_centroid(&render(&looking(gaze)), 0);
        let (dx, dy) = (x - rest_x, y - rest_y);
        let moved_x = if min_dx >= 0.0 {
            dx >= min_dx
        } else {
            dx <= min_dx
        };
        let moved_y = if min_dy >= 0.0 {
            dy >= min_dy
        } else {
            dy <= min_dy
        };
        assert!(moved_x && moved_y, "{name}: iris moved by ({dx}, {dy})");
    }
}

#[test]
fn vergence_turns_the_irises_towards_each_other() {
    let on_axis = EyesConfig {
        eyes_offset_x_m: 0.0,
        eyes_offset_y_m: 0.0,
        eyes_height_m: 1.6,
        ..EyesConfig::DEFAULT
    };
    let gaze = gaze_angles(&on_axis, 0.3).map(|angles| gaze_point(angles, on_axis.max_gaze_deg));
    let framebuffer = render(&Pose {
        gaze,
        ..Pose::RESTING
    });
    let (left_x, right_x) = (
        iris_centroid(&framebuffer, 0).0,
        iris_centroid(&framebuffer, 1).0,
    );
    assert!(
        left_x > 34.0 && right_x < 94.0,
        "left {left_x} right {right_x}"
    );
}

#[test]
fn a_full_blink_blacks_out_the_panel() {
    let framebuffer = render(&Pose {
        blink: 1.0,
        ..Pose::RESTING
    });
    assert_eq!(lit(&framebuffer), 0);
}

#[test]
fn closing_lids_light_fewer_pixels() {
    let lit_at = |blink: f32| {
        lit(&render(&Pose {
            blink,
            ..Pose::RESTING
        }))
    };
    assert!(lit_at(0.0) > lit_at(0.5) && lit_at(0.5) > lit_at(0.9) && lit_at(0.9) > 0);
}

#[test]
fn squint_tilts_the_lids_into_a_frown() {
    let framebuffer = render(&Pose {
        squint: 0.5,
        ..Pose::RESTING
    });
    let top_lit_row = |x: u32| {
        (0..64)
            .find(|&y| framebuffer.pixel(x, y) != Rgb888::BLACK)
            .unwrap_or(64)
    };
    // Screen-left eye: the inner (nose) corner is on the right, so its lid sits lower there.
    assert!(
        top_lit_row(50) > top_lit_row(14),
        "inner {} outer {}",
        top_lit_row(50),
        top_lit_row(14)
    );
}

#[test]
fn dilation_enlarges_the_pupil() {
    let pupil_pixels = |dilation: f32| {
        let framebuffer = render(&Pose {
            dilation,
            ..Pose::RESTING
        });
        (0..64)
            .flat_map(|y| (16..48).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                framebuffer.pixel(x, y) == Rgb888::BLACK
                    && (y as i32 - 32).abs() < 10
                    && (x as i32 - 32).abs() < 10
            })
            .count()
    };
    assert!(pupil_pixels(1.0) > pupil_pixels(0.0) + 100);
}

#[test]
fn out_of_range_pose_values_render_without_panicking() {
    let wild = Pose {
        gaze: [Vec2::new(f32::NAN, 3.0), Vec2::new(-40.0, f32::INFINITY)],
        blink: -2.0,
        squint: f32::NAN,
        dilation: 9.0,
        ..Pose::RESTING
    };
    assert!(lit(&render(&wild)) > 0);
}

#[test]
fn renders_to_rgb565_targets() {
    struct Panel565(Vec<Rgb565>);
    impl OriginDimensions for Panel565 {
        fn size(&self) -> Size {
            Size::new(128, 64)
        }
    }
    impl DrawTarget for Panel565 {
        type Color = Rgb565;
        type Error = core::convert::Infallible;
        fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
            &mut self,
            pixels: I,
        ) -> Result<(), Self::Error> {
            for Pixel(point, colour) in pixels {
                self.0[point.y as usize * 128 + point.x as usize] = colour;
            }
            Ok(())
        }
    }
    let mut panel = Panel565(vec![Rgb565::BLACK; 128 * 64]);
    draw(&Pose::RESTING, &mut panel).unwrap();
    let lit_565 = panel
        .0
        .iter()
        .filter(|&&pixel| pixel != Rgb565::BLACK)
        .count();
    assert_eq!(lit_565, lit(&render(&Pose::RESTING)));
}

/// Golden hashes pin the exact pixels. If a change is intentional, update the hash in the same commit
/// (and the matching value in eyes-web's smoke test, which proves the wasm build renders identically).
#[test]
fn golden_frames() {
    let config = EyesConfig::DEFAULT;
    let tracking = |distance_m: f32, squint: f32, dilation: f32| Pose {
        gaze: gaze_angles(&config, distance_m)
            .map(|angles| gaze_point(angles, config.max_gaze_deg)),
        blink: 0.0,
        squint,
        dilation,
        ..Pose::RESTING
    };
    let cases = [
        ("resting", Pose::RESTING, 0x1958_2E79),
        ("visitor at 3 m", tracking(3.0, 0.0, 0.4), 0x56CA_4AFA),
        ("visitor at 1.5 m", tracking(1.5, 0.0, 0.7), 0xEAAA_2F61),
        (
            "visitor at 0.5 m, squinting",
            tracking(0.5, 0.4, 0.95),
            0x3A95_395F,
        ),
        (
            "mid-blink",
            Pose {
                blink: 0.6,
                ..Pose::RESTING
            },
            0xB5C2_1129,
        ),
    ];
    let mismatches: Vec<String> = cases
        .iter()
        .filter_map(|(name, pose, want)| {
            let got = fnv1a(&render(pose));
            (got != *want).then(|| format!("{name}: 0x{got:08X}"))
        })
        .collect();
    assert!(mismatches.is_empty(), "golden mismatch: {mismatches:?}");
}

fn evil(pose: Pose) -> Pose {
    Pose {
        look: Look::Evil,
        brow: 1.0,
        ..pose
    }
}

#[test]
fn evil_eyes_have_a_dark_red_sclera_and_a_glowing_red_pupil() {
    let framebuffer = render(&evil(Pose::RESTING));
    let sclera = framebuffer.pixel(50, 32);
    let pupil = framebuffer.pixel(32, 32);
    let dark_red = sclera.r() > 0 && sclera.r() < 100 && sclera.g() == 0 && sclera.b() == 0;
    let glowing = pupil.r() > 200 && pupil.r() > pupil.g();
    assert!(dark_red && glowing, "sclera {sclera:?} pupil {pupil:?}");
}

#[test]
fn the_evil_brow_lowers_the_inner_corners_without_lifting_the_outer_ones() {
    let normal = render(&Pose::RESTING);
    let scowl = render(&evil(Pose::RESTING));
    let top_lit_row = |framebuffer: &Framebuffer, x: u32| {
        (0..64)
            .find(|&y| framebuffer.pixel(x, y) != Rgb888::BLACK)
            .unwrap_or(64)
    };
    // Screen-left eye: inner (nose) corner on the right; the right eye mirrors it.
    let inner_lowered = top_lit_row(&scowl, 50) > top_lit_row(&scowl, 14)
        && top_lit_row(&scowl, 77) > top_lit_row(&scowl, 113);
    let outer_not_lifted = top_lit_row(&scowl, 14) >= top_lit_row(&normal, 14)
        && top_lit_row(&scowl, 113) >= top_lit_row(&normal, 113);
    assert!(inner_lowered && outer_not_lifted);
}

#[test]
fn evil_eyes_draw_less_current_than_normal_ones() {
    // Total channel intensity is proportional to the panel's LED current at a given brightness.
    let intensity = |framebuffer: &Framebuffer| -> u32 {
        framebuffer
            .pixels()
            .iter()
            .map(|pixel| u32::from(pixel.r()) + u32::from(pixel.g()) + u32::from(pixel.b()))
            .sum()
    };
    let config = EyesConfig::DEFAULT;
    let poses = [0.4, 1.5, 3.0].map(|distance_m| Pose {
        gaze: gaze_angles(&config, distance_m)
            .map(|angles| gaze_point(angles, config.max_gaze_deg)),
        dilation: 1.0,
        ..Pose::RESTING
    });
    for pose in poses {
        let (normal, scary) = (intensity(&render(&pose)), intensity(&render(&evil(pose))));
        assert!(scary < normal, "evil {scary} vs normal {normal}");
    }
}

#[test]
fn a_non_finite_brow_renders_without_panicking() {
    let pose = Pose {
        brow: f32::NAN,
        ..evil(Pose::RESTING)
    };
    assert!(lit(&render(&pose)) > 0);
}

/// Evil goldens are separate from the normal ones above, which must never change for evil-mode work.
#[test]
fn evil_golden_frames() {
    let config = EyesConfig::DEFAULT;
    let tracking = |distance_m: f32, brow: f32| Pose {
        gaze: gaze_angles(&config, distance_m)
            .map(|angles| gaze_point(angles, config.max_gaze_deg)),
        dilation: 0.7,
        look: Look::Evil,
        brow,
        ..Pose::RESTING
    };
    let cases = [
        ("evil, visitor at 1.5 m", tracking(1.5, 1.0), 0x53EA_71F5),
        ("evil, brow half formed", tracking(1.5, 0.5), 0x2AC9_2259),
    ];
    let mismatches: Vec<String> = cases
        .iter()
        .filter_map(|(name, pose, want)| {
            let got = fnv1a(&render(pose));
            (got != *want).then(|| format!("{name}: 0x{got:08X}"))
        })
        .collect();
    assert!(mismatches.is_empty(), "golden mismatch: {mismatches:?}");
}

#[test]
fn a_shut_lid_is_shut_whatever_the_squint_brow_look_or_gaze() {
    for look in [Look::Normal, Look::Evil] {
        for squint_step in 0..=8 {
            for brow_step in 0..=4 {
                for gaze in [Vec2::ZERO, Vec2::new(0.9, 0.3), Vec2::new(-0.9, -0.3)] {
                    let pose = Pose {
                        gaze: [gaze; 2],
                        blink: 1.0,
                        squint: squint_step as f32 * 0.05,
                        brow: brow_step as f32 * 0.25,
                        look,
                        ..Pose::RESTING
                    };
                    assert_eq!(lit(&render(&pose)), 0, "{pose:?}");
                }
            }
        }
    }
}

/// Tracks a squinting visitor at the door, asks for `to` at `request_ms`, and returns the rendered switch
/// frame (or `None` if the look never changed).
fn switch_frame(to: Look, request_ms: u64, frame_ms: u64) -> Option<Framebuffer> {
    use eyes_core::{Eyes, Reading, SplitMix64};
    let mut eyes = Eyes::new(EyesConfig::DEFAULT, SplitMix64::new(3), 0);
    if to == Look::Normal {
        eyes.request_look(Look::Evil, 0);
    }
    let mut now_ms = 0;
    let mut previous = Look::Normal;
    while now_ms < request_ms + 2000 {
        if now_ms % 100 < frame_ms {
            eyes.observe(Reading::Distance(0.45), now_ms);
        }
        if (request_ms..request_ms + frame_ms).contains(&now_ms) {
            eyes.request_look(to, now_ms);
        }
        let pose = eyes.tick(now_ms);
        if now_ms > request_ms && pose.look == to && previous != to {
            return Some(render(&pose));
        }
        previous = pose.look;
        now_ms += frame_ms;
    }
    None
}

#[test]
fn the_look_switch_frame_shows_nothing_even_for_a_squinting_visitor() {
    // Requests at many moments relative to the eyes' own blinks, both ways, at 200 and 30 fps.
    for to in [Look::Evil, Look::Normal] {
        for frame_ms in [5, 33] {
            for request_ms in (3000..6000).step_by(137) {
                let frame = switch_frame(to, request_ms, frame_ms).expect("the look switched");
                assert_eq!(
                    lit(&frame),
                    0,
                    "to {to:?} at {request_ms} ms, {frame_ms} ms frames"
                );
            }
        }
    }
}

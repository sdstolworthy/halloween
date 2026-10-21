//! Tunable configuration: the installation geometry plus a few behaviour knobs.
//!
//! Every key is described once in [`Param`] (JSON key, range, unit), and that table drives validation,
//! the JSON merge used by the retained config topic, and the emulator's controls.

use alloc::string::String;
use core::fmt;

use crate::layout::EYE_SPACING_PX;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyesConfig {
    /// Lateral offset of the panel centre from the sensor's aim axis (+x).
    pub eyes_offset_x_m: f32,
    /// Offset of the panel centre along the walkway; negative is behind the sensor (towards the door).
    pub eyes_offset_y_m: f32,
    /// Panel centre height above the ground.
    pub eyes_height_m: f32,
    /// Direction the panel faces; 0 is straight down the walkway (+y), positive turns it towards +x.
    pub panel_yaw_deg: f32,
    /// Positive tilts the panel to face upwards.
    pub panel_pitch_deg: f32,
    /// Assumed height of the visitor's face.
    pub face_height_m: f32,
    /// LED pitch; sets the physical spacing of the two eyes.
    pub pixel_pitch_mm: f32,
    /// Gaze angle at which the pupil reaches the edge of its travel.
    pub max_gaze_deg: f32,
    /// Readings at or beyond this mean nobody is there. porch-sensor reports "nobody" as 4.5 m.
    pub max_range_m: f32,
    /// With no reading for this long the eyes go idle, so a dead sensor or broker never freezes them.
    pub stale_timeout_s: f32,
    /// Panel brightness, 1..=255. Kept low by default: a 128x64 panel at full white draws several amps.
    pub brightness: u8,
}

impl EyesConfig {
    pub const DEFAULT: Self = Self {
        eyes_offset_x_m: 0.6,
        eyes_offset_y_m: -0.3,
        eyes_height_m: 1.5,
        panel_yaw_deg: 0.0,
        panel_pitch_deg: 0.0,
        face_height_m: 1.6,
        pixel_pitch_mm: 3.0,
        max_gaze_deg: 55.0,
        max_range_m: 4.5,
        stale_timeout_s: 2.0,
        brightness: 64,
    };

    /// Physical distance between the two eye centres on the panel.
    pub fn eye_spacing_m(&self) -> f32 {
        EYE_SPACING_PX * self.pixel_pitch_mm / 1000.0
    }

    pub fn get(&self, param: Param) -> f32 {
        match param {
            Param::EyesOffsetX => self.eyes_offset_x_m,
            Param::EyesOffsetY => self.eyes_offset_y_m,
            Param::EyesHeight => self.eyes_height_m,
            Param::PanelYaw => self.panel_yaw_deg,
            Param::PanelPitch => self.panel_pitch_deg,
            Param::FaceHeight => self.face_height_m,
            Param::PixelPitch => self.pixel_pitch_mm,
            Param::MaxGaze => self.max_gaze_deg,
            Param::MaxRange => self.max_range_m,
            Param::StaleTimeout => self.stale_timeout_s,
            Param::Brightness => f32::from(self.brightness),
        }
    }

    /// A copy with `param` set to `value`, or an error if the value is outside the param's range.
    pub fn with(mut self, param: Param, value: f32) -> Result<Self, ConfigError> {
        let spec = param.spec();
        let in_range = value.is_finite() && (spec.min..=spec.max).contains(&value);
        if !in_range || (spec.integer && libm::truncf(value) != value) {
            return Err(ConfigError::OutOfRange { param, value });
        }
        match param {
            Param::EyesOffsetX => self.eyes_offset_x_m = value,
            Param::EyesOffsetY => self.eyes_offset_y_m = value,
            Param::EyesHeight => self.eyes_height_m = value,
            Param::PanelYaw => self.panel_yaw_deg = value,
            Param::PanelPitch => self.panel_pitch_deg = value,
            Param::FaceHeight => self.face_height_m = value,
            Param::PixelPitch => self.pixel_pitch_mm = value,
            Param::MaxGaze => self.max_gaze_deg = value,
            Param::MaxRange => self.max_range_m = value,
            Param::StaleTimeout => self.stale_timeout_s = value,
            Param::Brightness => self.brightness = value as u8,
        }
        Ok(self)
    }

    /// Apply a JSON object with any subset of the [`Param`] keys over `self`. All or nothing: one bad key
    /// or value rejects the whole object. An empty payload (how a retained message is cleared) yields
    /// `self` unchanged.
    pub fn merged_with_json(&self, json: &[u8]) -> Result<Self, ConfigError> {
        if json.iter().all(u8::is_ascii_whitespace) {
            return Ok(*self);
        }
        let value: serde_json::Value =
            serde_json::from_slice(json).map_err(|_| ConfigError::InvalidJson)?;
        let object = value.as_object().ok_or(ConfigError::NotAnObject)?;
        object.iter().try_fold(*self, |config, (key, value)| {
            let param = Param::from_key(key).ok_or_else(|| ConfigError::UnknownKey(key.clone()))?;
            let number = value.as_f64().ok_or(ConfigError::NotANumber(param))?;
            config.with(param, number as f32)
        })
    }
}

impl Default for EyesConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    EyesOffsetX,
    EyesOffsetY,
    EyesHeight,
    PanelYaw,
    PanelPitch,
    FaceHeight,
    PixelPitch,
    MaxGaze,
    MaxRange,
    StaleTimeout,
    Brightness,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    pub key: &'static str,
    pub min: f32,
    pub max: f32,
    /// Suggested UI increment.
    pub step: f32,
    pub integer: bool,
}

const fn spec(key: &'static str, min: f32, max: f32, step: f32) -> ParamSpec {
    ParamSpec {
        key,
        min,
        max,
        step,
        integer: false,
    }
}

impl Param {
    pub const ALL: [Param; 11] = [
        Param::EyesOffsetX,
        Param::EyesOffsetY,
        Param::EyesHeight,
        Param::PanelYaw,
        Param::PanelPitch,
        Param::FaceHeight,
        Param::PixelPitch,
        Param::MaxGaze,
        Param::MaxRange,
        Param::StaleTimeout,
        Param::Brightness,
    ];

    pub const fn spec(self) -> ParamSpec {
        match self {
            Param::EyesOffsetX => spec("eyes_offset_x_m", -10.0, 10.0, 0.05),
            Param::EyesOffsetY => spec("eyes_offset_y_m", -10.0, 10.0, 0.05),
            Param::EyesHeight => spec("eyes_height_m", 0.0, 5.0, 0.05),
            Param::PanelYaw => spec("panel_yaw_deg", -180.0, 180.0, 1.0),
            Param::PanelPitch => spec("panel_pitch_deg", -60.0, 60.0, 1.0),
            Param::FaceHeight => spec("face_height_m", 0.3, 2.5, 0.05),
            Param::PixelPitch => spec("pixel_pitch_mm", 1.0, 10.0, 0.1),
            Param::MaxGaze => spec("max_gaze_deg", 10.0, 85.0, 1.0),
            Param::MaxRange => spec("max_range_m", 0.5, 10.0, 0.05),
            Param::StaleTimeout => spec("stale_timeout_s", 0.2, 60.0, 0.1),
            Param::Brightness => ParamSpec {
                integer: true,
                ..spec("brightness", 1.0, 255.0, 1.0)
            },
        }
    }

    pub fn from_key(key: &str) -> Option<Param> {
        Param::ALL.into_iter().find(|param| param.spec().key == key)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConfigError {
    InvalidJson,
    NotAnObject,
    UnknownKey(String),
    NotANumber(Param),
    OutOfRange { param: Param, value: f32 },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson => f.write_str("config is not valid JSON"),
            Self::NotAnObject => f.write_str("config must be a JSON object"),
            Self::UnknownKey(key) => write!(f, "unknown config key {key:?}"),
            Self::NotANumber(param) => write!(f, "{} must be a number", param.spec().key),
            Self::OutOfRange { param, value } => {
                let spec = param.spec();
                let kind = if spec.integer { "an integer " } else { "" };
                write!(
                    f,
                    "{} = {value} is out of range; expected {kind}{}..={}",
                    spec.key, spec.min, spec.max
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_within_their_own_ranges() {
        let defaults = EyesConfig::DEFAULT;
        for param in Param::ALL {
            assert_eq!(
                defaults.with(param, defaults.get(param)),
                Ok(defaults),
                "{param:?}"
            );
        }
    }

    #[test]
    fn keys_round_trip_through_from_key() {
        for param in Param::ALL {
            assert_eq!(Param::from_key(param.spec().key), Some(param));
        }
    }

    #[test]
    fn eye_spacing_follows_pixel_pitch() {
        let config = EyesConfig::DEFAULT.with(Param::PixelPitch, 2.5).unwrap();
        assert!((config.eye_spacing_m() - 0.16).abs() < 1e-6);
    }

    #[test]
    fn with_validates_ranges() {
        let cases = [
            (Param::EyesHeight, 5.0, true),
            (Param::EyesHeight, 5.01, false),
            (Param::EyesHeight, -0.1, false),
            (Param::PanelYaw, -180.0, true),
            (Param::MaxGaze, 9.0, false),
            (Param::FaceHeight, f32::NAN, false),
            (Param::EyesOffsetX, f32::INFINITY, false),
            (Param::Brightness, 255.0, true),
            (Param::Brightness, 0.0, false),
            (Param::Brightness, 12.5, false),
        ];
        for (param, value, ok) in cases {
            assert_eq!(
                EyesConfig::DEFAULT.with(param, value).is_ok(),
                ok,
                "{param:?} = {value}"
            );
        }
    }

    #[test]
    fn json_merge_sets_only_the_given_keys() {
        let merged = EyesConfig::DEFAULT
            .merged_with_json(br#"{"eyes_height_m": 2.1, "brightness": 30}"#)
            .unwrap();
        let expected = EyesConfig {
            eyes_height_m: 2.1,
            brightness: 30,
            ..EyesConfig::DEFAULT
        };
        assert_eq!(merged, expected);
    }

    #[test]
    fn empty_payload_keeps_the_base_config() {
        for payload in [&b""[..], b"  \n"] {
            assert_eq!(
                EyesConfig::DEFAULT.merged_with_json(payload),
                Ok(EyesConfig::DEFAULT)
            );
        }
    }

    #[test]
    fn json_merge_rejects_bad_input() {
        let cases: [(&[u8], ConfigError); 6] = [
            (b"{", ConfigError::InvalidJson),
            (b"[1, 2]", ConfigError::NotAnObject),
            (b"1.5", ConfigError::NotAnObject),
            (
                br#"{"eye_height_m": 2}"#,
                ConfigError::UnknownKey("eye_height_m".into()),
            ),
            (
                br#"{"eyes_height_m": "2"}"#,
                ConfigError::NotANumber(Param::EyesHeight),
            ),
            (
                br#"{"max_gaze_deg": 120}"#,
                ConfigError::OutOfRange {
                    param: Param::MaxGaze,
                    value: 120.0,
                },
            ),
        ];
        for (payload, want) in cases {
            assert_eq!(
                EyesConfig::DEFAULT.merged_with_json(payload),
                Err(want),
                "{:?}",
                core::str::from_utf8(payload)
            );
        }
    }

    #[test]
    fn json_merge_is_all_or_nothing() {
        let result =
            EyesConfig::DEFAULT.merged_with_json(br#"{"eyes_height_m": 2.0, "max_gaze_deg": 120}"#);
        assert!(result.is_err());
    }

    #[test]
    fn errors_name_the_key_and_range() {
        let error = EyesConfig::DEFAULT
            .with(Param::Brightness, 300.0)
            .unwrap_err();
        assert_eq!(
            alloc::string::ToString::to_string(&error),
            "brightness = 300 is out of range; expected an integer 1..=255"
        );
    }
}

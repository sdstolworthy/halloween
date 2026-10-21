//! Parsing of the distance topic's payload: decimal metres as plain text, or the literal `none`.

use core::fmt;

/// porch-sensor's farthest reading. It reports no echo, or anything beyond its own Max range, as `none`.
pub const SENSOR_MAX_RANGE_M: f32 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    /// A measured distance in metres. Values at or beyond the configured max range still mean "nobody";
    /// that threshold is configuration, so [`crate::Eyes`] applies it rather than the parser.
    Distance(f32),
    Nobody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadError {
    NotUtf8,
    NotANumber,
    NotFinite,
    Negative,
}

impl fmt::Display for PayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotUtf8 => "payload is not UTF-8",
            Self::NotANumber => "payload is neither a number of metres nor `none`",
            Self::NotFinite => "distance is not finite",
            Self::Negative => "distance is negative",
        })
    }
}

pub fn parse_distance(payload: &[u8]) -> Result<Reading, PayloadError> {
    let text = core::str::from_utf8(payload)
        .map_err(|_| PayloadError::NotUtf8)?
        .trim();
    if text.eq_ignore_ascii_case("none") {
        return Ok(Reading::Nobody);
    }
    let metres: f32 = text.parse().map_err(|_| PayloadError::NotANumber)?;
    if !metres.is_finite() {
        Err(PayloadError::NotFinite)
    } else if metres < 0.0 {
        Err(PayloadError::Negative)
    } else {
        Ok(Reading::Distance(metres))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_payloads() {
        let cases: [(&[u8], Result<Reading, PayloadError>); 14] = [
            (b"1.83", Ok(Reading::Distance(1.83))),
            (b"4.50", Ok(Reading::Distance(4.5))),
            (b"0", Ok(Reading::Distance(0.0))),
            (b" 2.5\n", Ok(Reading::Distance(2.5))),
            (b"3", Ok(Reading::Distance(3.0))),
            (b"none", Ok(Reading::Nobody)),
            (b"NONE", Ok(Reading::Nobody)),
            (b"", Err(PayloadError::NotANumber)),
            (b"1.2m", Err(PayloadError::NotANumber)),
            (b"{\"d\":1}", Err(PayloadError::NotANumber)),
            (b"NaN", Err(PayloadError::NotFinite)),
            (b"inf", Err(PayloadError::NotFinite)),
            (b"-0.5", Err(PayloadError::Negative)),
            (&[0xff, 0xfe], Err(PayloadError::NotUtf8)),
        ];
        for (payload, want) in cases {
            assert_eq!(parse_distance(payload), want, "{payload:?}");
        }
    }
}

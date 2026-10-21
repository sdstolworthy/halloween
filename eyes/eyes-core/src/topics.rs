//! The MQTT contract. See `docs/specs/2026-10-20-tracking-eyes-design.md`.

/// Plain-text metres from porch-sensor, ~10 Hz, QoS 0, not retained.
pub const DISTANCE: &str = "halloween/porch/distance";
/// Retained JSON object of config keys, merged over the provisioned defaults.
pub const CONFIG: &str = "halloween/eyes/config";
/// Retained availability: [`ONLINE`] on connect, [`OFFLINE`] via the last will.
pub const STATUS: &str = "halloween/eyes/status";

pub const ONLINE: &[u8] = b"online";
pub const OFFLINE: &[u8] = b"offline";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Topic {
    Distance,
    Config,
}

pub fn route(topic: &str) -> Option<Topic> {
    match topic {
        DISTANCE => Some(Topic::Distance),
        CONFIG => Some(Topic::Config),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_known_topics_and_ignores_others() {
        let cases = [
            (DISTANCE, Some(Topic::Distance)),
            (CONFIG, Some(Topic::Config)),
            (STATUS, None),
            ("halloween/porch/distance/extra", None),
            ("", None),
        ];
        for (topic, want) in cases {
            assert_eq!(route(topic), want, "{topic:?}");
        }
    }
}

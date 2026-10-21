//! Settings baked in at build time from `provision.toml` by build.rs.

use eyes_core::EyesConfig;
use log::error;

#[derive(Clone, Copy)]
pub struct WifiCredentials {
    pub ssid: &'static str,
    pub password: &'static str,
}

#[derive(Clone, Copy)]
pub struct MqttSettings {
    pub url: &'static str,
    pub username: Option<&'static str>,
    pub password: Option<&'static str>,
}

pub struct Provisioning {
    pub wifi: Option<WifiCredentials>,
    pub mqtt: Option<MqttSettings>,
    pub defaults: EyesConfig,
}

impl Provisioning {
    pub fn baked() -> Self {
        let non_empty = |value: Option<&'static str>| value.filter(|text| !text.is_empty());
        let wifi = non_empty(option_env!("EYES_WIFI_SSID")).map(|ssid| WifiCredentials {
            ssid,
            password: option_env!("EYES_WIFI_PASSWORD").unwrap_or_default(),
        });
        let mqtt = non_empty(option_env!("EYES_MQTT_URL")).map(|url| MqttSettings {
            url,
            username: non_empty(option_env!("EYES_MQTT_USERNAME")),
            password: non_empty(option_env!("EYES_MQTT_PASSWORD")),
        });
        // build.rs already validated this, so a failure here means the two disagree; never brick over it.
        let defaults = EyesConfig::DEFAULT
            .merged_with_json(
                option_env!("EYES_DEFAULT_CONFIG")
                    .unwrap_or_default()
                    .as_bytes(),
            )
            .unwrap_or_else(|reason| {
                error!("baked-in eyes defaults rejected ({reason}); using built-in defaults");
                EyesConfig::DEFAULT
            });
        Self {
            wifi,
            mqtt,
            defaults,
        }
    }
}

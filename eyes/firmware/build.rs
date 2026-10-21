//! Bakes the gitignored `provision.toml` (see `provision.example.toml`) into the image as `EYES_*`
//! compile-time environment variables, read back by `src/provision.rs`. A missing file is fine: the eyes
//! then run idle and offline. A malformed one fails the build rather than shipping a broken device.

use toml::{Table, Value};

const PROVISION_FILE: &str = "provision.toml";

type Keys = &'static [(&'static str, Option<usize>)];

/// Allowed keys per section, with the longest value (bytes) the firmware can hold, if limited. The Wi-Fi
/// driver's fixed buffers would otherwise silently turn an over-long value into an empty one.
const SECTIONS: [(&str, Keys); 2] = [
    ("wifi", &[("ssid", Some(32)), ("password", Some(64))]),
    (
        "mqtt",
        &[("url", None), ("username", None), ("password", None)],
    ),
];

fn main() {
    embuild::espidf::sysenv::output();

    println!("cargo:rerun-if-changed={PROVISION_FILE}");
    let Ok(text) = std::fs::read_to_string(PROVISION_FILE) else {
        println!(
            "cargo:warning=no {PROVISION_FILE}: building without Wi-Fi/MQTT settings; the eyes will run idle and offline"
        );
        return;
    };
    let provision: Table = text
        .parse()
        .unwrap_or_else(|error| panic!("{PROVISION_FILE}: {error}"));

    for (name, value) in &provision {
        match name.as_str() {
            "eyes" => bake_eyes_defaults(value),
            _ => bake_section(name, value),
        }
    }
}

fn bake_section(name: &str, value: &Value) {
    let Some((_, allowed)) = SECTIONS.iter().find(|(section, _)| *section == name) else {
        panic!("{PROVISION_FILE}: unknown section [{name}]");
    };
    let table = value
        .as_table()
        .unwrap_or_else(|| panic!("{PROVISION_FILE}: [{name}] must be a table"));
    for (key, value) in table {
        let Some((_, max_len)) = allowed.iter().find(|(allowed_key, _)| allowed_key == key) else {
            panic!("{PROVISION_FILE}: unknown key {name}.{key}");
        };
        let text = value
            .as_str()
            .unwrap_or_else(|| panic!("{PROVISION_FILE}: {name}.{key} must be a string"));
        if let Some(max_len) = max_len {
            assert!(
                text.len() <= *max_len,
                "{PROVISION_FILE}: {name}.{key} is {} bytes; the most the firmware can use is {max_len}",
                text.len()
            );
        }
        assert!(
            !text.contains(['\n', '\r']),
            "{PROVISION_FILE}: {name}.{key} must be a single line"
        );
        println!(
            "cargo:rustc-env=EYES_{}_{}={text}",
            name.to_uppercase(),
            key.to_uppercase()
        );
    }
}

/// Validated with the same rules the device applies to the retained config topic.
fn bake_eyes_defaults(value: &Value) {
    let json = serde_json::to_string(value).expect("TOML values always serialise to JSON");
    if let Err(error) = eyes_core::EyesConfig::DEFAULT.merged_with_json(json.as_bytes()) {
        panic!("{PROVISION_FILE}: [eyes]: {error}");
    }
    println!("cargo:rustc-env=EYES_DEFAULT_CONFIG={json}");
}

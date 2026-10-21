import pytest

from witch_bridge.config import Config, ConfigError


@pytest.mark.parametrize(
    "env, expected",
    [
        ({"MQTT_HOST": "broker"}, Config(mqtt_host="broker")),
        (
            {"MQTT_HOST": "broker", "MQTT_PORT": "8883", "MQTT_USERNAME": "witch", "MQTT_PASSWORD": "pw"},
            Config(mqtt_host="broker", mqtt_port=8883, mqtt_username="witch", mqtt_password="pw"),
        ),
        ({"MQTT_HOST": "broker", "MQTT_PORT": "", "MQTT_USERNAME": ""}, Config(mqtt_host="broker")),
        (
            {"MQTT_HOST": "broker", "MPV_SOCKET": "/tmp/s", "WITCH_VIDEO": "/v.mp4", "DISCOVERY_PREFIX": "ha"},
            Config(mqtt_host="broker", mpv_socket="/tmp/s", video_path="/v.mp4", discovery_prefix="ha"),
        ),
    ],
)
def test_from_env(env, expected):
    assert Config.from_env(env) == expected


@pytest.mark.parametrize(
    "env",
    [
        {},
        {"MQTT_HOST": ""},
        {"MQTT_HOST": "broker", "MQTT_PORT": "mqtt"},
    ],
)
def test_from_env_rejects_invalid(env):
    with pytest.raises(ConfigError):
        Config.from_env(env)

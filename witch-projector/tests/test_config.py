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
            {"MQTT_HOST": "broker", "MPV_SOCKET": "/tmp/s", "WITCH_VIDEOS_DIR": "/v", "DISCOVERY_PREFIX": "ha"},
            Config(mqtt_host="broker", mpv_socket="/tmp/s", videos_dir="/v", discovery_prefix="ha"),
        ),
        (
            {"MQTT_HOST": "broker", "WITCH_VIDEOS": "witch1, witch2,ghost"},
            Config(mqtt_host="broker", videos=("witch1", "witch2", "ghost")),
        ),
        ({"MQTT_HOST": "broker", "WITCH_VIDEOS": ""}, Config(mqtt_host="broker")),
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
        {"MQTT_HOST": "broker", "WITCH_VIDEOS": "witch 1"},
        {"MQTT_HOST": "broker", "WITCH_VIDEOS": "witch/1"},
        {"MQTT_HOST": "broker", "WITCH_VIDEOS": "witch1,,ghost"},
        {"MQTT_HOST": "broker", "WITCH_VIDEOS": "ghost,ghost"},
    ],
)
def test_from_env_rejects_invalid(env):
    with pytest.raises(ConfigError):
        Config.from_env(env)


def test_video_paths_live_in_the_videos_dir():
    config = Config(mqtt_host="broker", videos_dir="/opt/v", videos=("witch1", "ghost"))
    assert config.video_paths() == {"witch1": "/opt/v/witch1.mp4", "ghost": "/opt/v/ghost.mp4"}

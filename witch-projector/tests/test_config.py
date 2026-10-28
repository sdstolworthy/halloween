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
        ({"MQTT_HOST": "broker", "WITCH_IDLE_VIDEO": "happy"}, Config(mqtt_host="broker", idle_video="happy")),
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
        {"MQTT_HOST": "broker", "WITCH_IDLE_VIDEO": "happy face"},
        {"MQTT_HOST": "broker", "WITCH_VIDEOS": "witch1,happy", "WITCH_IDLE_VIDEO": "happy"},
    ],
)
def test_from_env_rejects_invalid(env):
    with pytest.raises(ConfigError):
        Config.from_env(env)


def test_video_paths_live_in_the_videos_dir():
    config = Config(mqtt_host="broker", videos_dir="/opt/v", videos=("witch1", "ghost"))
    assert config.video_paths() == {"witch1": "/opt/v/witch1.mp4", "ghost": "/opt/v/ghost.mp4"}


@pytest.mark.parametrize("idle_video, expected", [("happy", "/opt/v/happy.mp4"), (None, None)])
def test_idle_video_path_lives_in_the_videos_dir(idle_video, expected):
    assert Config(mqtt_host="broker", videos_dir="/opt/v", idle_video=idle_video).idle_video_path() == expected

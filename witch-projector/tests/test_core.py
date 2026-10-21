import json

import pytest

from fakes import FakePlayer, FakePublisher
from witch_bridge.core import Bridge, Topics, discovery_messages

TOPICS = Topics()
VIDEO = "/videos/witch.mp4"


@pytest.fixture
def mqtt():
    return FakePublisher()


@pytest.fixture
def mpv():
    return FakePlayer()


@pytest.fixture
def bridge(mqtt, mpv):
    return Bridge(mqtt, mpv, TOPICS, VIDEO)


def idle_active(value):
    return {"event": "property-change", "id": 1, "name": "idle-active", "data": value}


@pytest.mark.parametrize(
    "payload, expected",
    [
        ("play", [("loadfile", VIDEO, "replace")]),
        ("stop", [("stop",)]),
        ("dance", []),
        ("", []),
    ],
)
def test_command_maps_to_mpv_request(bridge, mpv, payload, expected):
    bridge.command(payload)
    assert mpv.commands == expected


def test_mpv_connect_observes_idle_active(bridge, mpv):
    bridge.mpv_connected()
    assert mpv.commands == [("observe_property", 1, "idle-active")]


@pytest.mark.parametrize("idle, expected", [(True, "OFF"), (False, "ON")])
def test_idle_active_drives_playing(bridge, mqtt, idle, expected):
    bridge.mpv_message(idle_active(idle))
    assert mqtt.payloads_for(TOPICS.playing) == [expected]


@pytest.mark.parametrize(
    "message",
    [
        {"event": "property-change", "id": 2, "name": "pause", "data": True},
        {"event": "end-file", "reason": "eof"},
        {"request_id": 0, "error": "success"},
        {"request_id": 0, "error": "invalid parameter"},
    ],
)
def test_unrelated_mpv_messages_publish_nothing(bridge, mqtt, message):
    bridge.mpv_message(message)
    assert mqtt.published == []


@pytest.mark.parametrize(
    "events, expected",
    [
        (["mqtt_connected"], ["offline"]),
        (["mqtt_connected", "mpv_connected"], ["offline", "online"]),
        (["mpv_connected", "mpv_disconnected", "mqtt_connected"], ["online", "offline", "offline"]),
        (["mpv_connected", "mqtt_connected"], ["online", "online"]),
    ],
)
def test_availability_follows_mpv_connection(bridge, mqtt, events, expected):
    for event in events:
        getattr(bridge, event)()
    assert mqtt.payloads_for(TOPICS.availability) == expected


def test_mqtt_reconnect_republishes_last_playing_state(bridge, mqtt):
    bridge.mpv_message(idle_active(False))
    bridge.mqtt_connected()
    assert mqtt.payloads_for(TOPICS.playing) == ["ON", "ON"]


def test_playing_is_not_published_once_mpv_is_lost(bridge, mqtt):
    bridge.mpv_message(idle_active(False))
    bridge.mpv_disconnected()
    bridge.mqtt_connected()
    assert mqtt.payloads_for(TOPICS.playing) == ["ON"]


def test_mqtt_connect_publishes_every_discovery_config(bridge, mqtt):
    bridge.mqtt_connected()
    discovery = {topic for topic, _ in discovery_messages(TOPICS)}
    assert discovery <= {topic for topic, _, _ in mqtt.published}


def test_everything_the_bridge_publishes_is_retained(bridge, mqtt):
    bridge.mpv_connected()
    bridge.mpv_message(idle_active(False))
    bridge.mqtt_connected()
    assert all(retain for _, _, retain in mqtt.published)


@pytest.mark.parametrize(
    "topic, expected",
    [
        ("homeassistant/button/witch_projector/play_witch/config",
         {"name": "Play witch", "command_topic": "witch_projector/command", "payload_press": "play"}),
        ("homeassistant/button/witch_projector/stop/config",
         {"name": "Stop", "command_topic": "witch_projector/command", "payload_press": "stop"}),
        ("homeassistant/binary_sensor/witch_projector/playing/config",
         {"name": "Playing", "state_topic": "witch_projector/playing"}),
    ],
)
def test_discovery_config(topic, expected):
    config = json.loads(dict(discovery_messages(TOPICS))[topic])
    assert expected.items() <= config.items()


@pytest.mark.parametrize("topic, config", discovery_messages(TOPICS))
def test_discovery_config_belongs_to_witch_projector_device(topic, config):
    parsed = json.loads(config)
    assert (parsed["device"]["name"], parsed["availability_topic"]) == ("Witch projector", TOPICS.availability)


def test_discovery_unique_ids_are_distinct():
    unique_ids = [json.loads(config)["unique_id"] for _, config in discovery_messages(TOPICS)]
    assert len(set(unique_ids)) == len(unique_ids)


def test_discovery_prefix_is_configurable():
    topics = Topics(discovery_prefix="ha")
    assert all(topic.startswith("ha/") for topic, _ in discovery_messages(topics))

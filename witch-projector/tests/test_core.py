import json

import pytest

from fakes import FakePlayer, FakePublisher
from witch_bridge.core import Bridge, Topics, discovery_messages

TOPICS = Topics()
VIDEOS = {"witch1": "/videos/witch1.mp4", "witch2": "/videos/witch2.mp4", "ghost": "/videos/ghost.mp4"}
NAMES = list(VIDEOS)
HAPPY = "/videos/happy.mp4"
LOOP_HAPPY = ("loadfile", HAPPY, "replace", -1, "loop-file=inf")


class FirstChoice:
    """Stands in for random.choice: picks the first candidate and records what it was offered."""

    def __init__(self):
        self.offered = []

    def __call__(self, candidates):
        self.offered.append(list(candidates))
        return candidates[0]


@pytest.fixture
def mqtt():
    return FakePublisher()


@pytest.fixture
def mpv():
    return FakePlayer()


@pytest.fixture
def choose():
    return FirstChoice()


@pytest.fixture
def bridge(mqtt, mpv, choose):
    return Bridge(mqtt, mpv, TOPICS, VIDEOS, choose, idle_video=HAPPY)


def idle_active(value):
    return {"event": "property-change", "id": 1, "name": "idle-active", "data": value}


def path(value):
    return {"event": "property-change", "id": 2, "name": "path", "data": value}


def end_file(reason):
    return {"event": "end-file", "reason": reason}


@pytest.mark.parametrize(
    "payload, expected",
    [
        ("play", [("loadfile", "/videos/witch1.mp4", "replace")]),
        ("play:witch2", [("loadfile", "/videos/witch2.mp4", "replace")]),
        ("play:ghost", [("loadfile", "/videos/ghost.mp4", "replace")]),
        ("play:zombie", []),
        ("play:", []),
        ("stop", [("stop",)]),
        ("dance", []),
        ("", []),
    ],
)
def test_command_maps_to_mpv_request(bridge, mpv, payload, expected):
    bridge.command(payload)
    assert mpv.commands == expected


@pytest.mark.parametrize(
    "commands, expected_offered",
    [
        (["play"], [NAMES]),
        (["play", "play"], [NAMES, ["witch2", "ghost"]]),
        (["play:ghost", "play"], [["witch1", "witch2"]]),
    ],
)
def test_random_scare_skips_the_last_video_played(bridge, choose, commands, expected_offered):
    for command in commands:
        bridge.command(command)
    assert choose.offered == expected_offered


def test_random_scare_with_one_video_still_plays_it(mqtt, mpv, choose):
    bridge = Bridge(mqtt, mpv, TOPICS, {"ghost": "/videos/ghost.mp4"}, choose)
    bridge.command("play")
    bridge.command("play")
    assert mpv.commands == [("loadfile", "/videos/ghost.mp4", "replace")] * 2


def test_random_scare_with_no_videos_does_nothing(mqtt, mpv, choose):
    bridge = Bridge(mqtt, mpv, TOPICS, {}, choose)
    bridge.command("play")
    assert mpv.commands == []


def test_mpv_connect_observes_idle_active_and_path(bridge, mpv):
    bridge.mpv_connected()
    assert mpv.commands == [("observe_property", 1, "idle-active"), ("observe_property", 2, "path")]


@pytest.mark.parametrize(
    "current, expected",
    [
        ("/videos/witch1.mp4", "ON"),
        ("/videos/ghost.mp4", "ON"),
        (HAPPY, "OFF"),
        (None, "OFF"),
    ],
)
def test_only_a_scare_video_counts_as_playing(bridge, mqtt, current, expected):
    bridge.mpv_message(path(current))
    assert mqtt.payloads_for(TOPICS.playing) == [expected]


@pytest.mark.parametrize(
    "messages, expected",
    [
        ([idle_active(True)], [LOOP_HAPPY]),
        ([idle_active(False)], []),
        # A happy.mp4 that fails to load would otherwise be retried every time mpv falls idle.
        ([idle_active(True), end_file("error"), idle_active(True)], [LOOP_HAPPY]),
        ([idle_active(True), end_file("eof"), idle_active(True)], [LOOP_HAPPY, LOOP_HAPPY]),
    ],
)
def test_idle_mpv_loops_the_happy_video(bridge, mpv, messages, expected):
    for message in messages:
        bridge.mpv_message(message)
    assert mpv.commands == expected


def test_a_failed_scare_does_not_stop_the_happy_loop(bridge, mpv):
    bridge.command("play:ghost")
    for message in (end_file("error"), idle_active(True)):
        bridge.mpv_message(message)
    assert mpv.commands[-1] == LOOP_HAPPY


def test_mpv_reconnect_retries_a_broken_happy_video(bridge, mpv):
    for message in (idle_active(True), end_file("error")):
        bridge.mpv_message(message)
    bridge.mpv_disconnected()
    bridge.mpv_connected()
    bridge.mpv_message(idle_active(True))
    assert mpv.commands[-1] == LOOP_HAPPY


def test_without_an_idle_video_mpv_stays_black(mqtt, mpv, choose):
    bridge = Bridge(mqtt, mpv, TOPICS, VIDEOS, choose)
    bridge.mpv_message(idle_active(True))
    assert mpv.commands == []


@pytest.mark.parametrize(
    "message",
    [
        {"event": "property-change", "id": 2, "name": "pause", "data": True},
        end_file("eof"),
        idle_active(False),
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
    bridge.mpv_message(path("/videos/witch1.mp4"))
    bridge.mqtt_connected()
    assert mqtt.payloads_for(TOPICS.playing) == ["ON", "ON"]


def test_playing_is_not_published_once_mpv_is_lost(bridge, mqtt):
    bridge.mpv_message(path("/videos/witch1.mp4"))
    bridge.mpv_disconnected()
    bridge.mqtt_connected()
    assert mqtt.payloads_for(TOPICS.playing) == ["ON"]


def test_mqtt_connect_publishes_every_discovery_config(bridge, mqtt):
    bridge.mqtt_connected()
    discovery = {topic for topic, _ in discovery_messages(TOPICS, NAMES)}
    assert discovery <= {topic for topic, _, _ in mqtt.published}


def test_everything_the_bridge_publishes_is_retained(bridge, mqtt):
    bridge.mpv_connected()
    bridge.mpv_message(path("/videos/witch1.mp4"))
    bridge.mqtt_connected()
    assert all(retain for _, _, retain in mqtt.published)


@pytest.mark.parametrize(
    "topic, expected",
    [
        ("homeassistant/button/witch_projector/play_witch/config",
         {"name": "Random scare", "command_topic": "witch_projector/command", "payload_press": "play"}),
        ("homeassistant/button/witch_projector/video_witch1/config",
         {"name": "Play witch1", "command_topic": "witch_projector/command", "payload_press": "play:witch1"}),
        ("homeassistant/button/witch_projector/video_ghost/config",
         {"name": "Play ghost", "command_topic": "witch_projector/command", "payload_press": "play:ghost"}),
        ("homeassistant/button/witch_projector/stop/config",
         {"name": "Stop", "command_topic": "witch_projector/command", "payload_press": "stop"}),
        ("homeassistant/binary_sensor/witch_projector/playing/config",
         {"name": "Playing", "state_topic": "witch_projector/playing"}),
    ],
)
def test_discovery_config(topic, expected):
    config = json.loads(dict(discovery_messages(TOPICS, NAMES))[topic])
    assert expected.items() <= config.items()


@pytest.mark.parametrize("topic, config", discovery_messages(TOPICS, NAMES))
def test_discovery_config_belongs_to_witch_projector_device(topic, config):
    parsed = json.loads(config)
    assert (parsed["device"]["name"], parsed["availability_topic"]) == ("Witch projector", TOPICS.availability)


def test_discovery_unique_ids_are_distinct():
    unique_ids = [json.loads(config)["unique_id"] for _, config in discovery_messages(TOPICS, NAMES)]
    assert len(set(unique_ids)) == len(unique_ids)


def test_discovery_prefix_is_configurable():
    topics = Topics(discovery_prefix="ha")
    assert all(topic.startswith("ha/") for topic, _ in discovery_messages(topics, NAMES))


def test_discovery_has_a_play_button_per_video_plus_random_scare():
    presses = [json.loads(config).get("payload_press") for _, config in discovery_messages(TOPICS, NAMES)]
    assert [press for press in presses if press and press.startswith("play")] == [
        "play", "play:witch1", "play:witch2", "play:ghost"]


def test_a_video_named_witch_does_not_collide_with_random_scare():
    unique_ids = [json.loads(config)["unique_id"] for _, config in discovery_messages(TOPICS, ["witch"])]
    assert len(set(unique_ids)) == len(unique_ids)

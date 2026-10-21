from types import SimpleNamespace

import pytest

from witch_bridge.core import Topics
from witch_bridge.mqtt import PahoMqtt

TOPICS = Topics()
SUCCESS = SimpleNamespace(is_failure=False)
REFUSED = SimpleNamespace(is_failure=True)


class FakePahoClient:
    def __init__(self):
        self.calls = []

    def __getattr__(self, name):
        if name.startswith("on_"):
            raise AttributeError(name)
        return lambda *args, **kwargs: self.calls.append((name, args, kwargs))

    def called(self, name):
        return [(args, kwargs) for called_name, args, kwargs in self.calls if called_name == name]


class Recorder:
    def __init__(self):
        self.connected = 0
        self.commands = []


@pytest.fixture
def client():
    return FakePahoClient()


@pytest.fixture
def recorder():
    return Recorder()


@pytest.fixture
def mqtt(client, recorder):
    def on_connected():
        recorder.connected += 1

    return PahoMqtt(client, TOPICS, on_connected=on_connected, on_command=recorder.commands.append)


def test_start_sets_a_retained_offline_last_will(mqtt, client):
    mqtt.start("broker", 1883, None, None)
    assert client.called("will_set") == [((TOPICS.availability, "offline"), {"qos": 1, "retain": True})]


@pytest.mark.parametrize(
    "username, password, expected",
    [
        ("witch", "secret", [(("witch", "secret"), {})]),
        (None, None, []),
    ],
)
def test_start_sets_credentials_only_when_given(mqtt, client, username, password, expected):
    mqtt.start("broker", 1883, username, password)
    assert client.called("username_pw_set") == expected


def test_start_connects_asynchronously_and_starts_the_loop(mqtt, client):
    mqtt.start("broker", 1884, None, None)
    assert [name for name, _, _ in client.calls][-2:] == ["connect_async", "loop_start"]


def test_connect_success_subscribes_to_commands(mqtt, client):
    client.on_connect(client, None, None, SUCCESS, None)
    assert client.called("subscribe") == [((TOPICS.command,), {"qos": 1})]


@pytest.mark.parametrize("reason_code, expected", [(SUCCESS, 1), (REFUSED, 0)])
def test_connect_notifies_only_on_success(mqtt, client, recorder, reason_code, expected):
    client.on_connect(client, None, None, reason_code, None)
    assert recorder.connected == expected


def test_message_payload_is_decoded_into_a_command(mqtt, client, recorder):
    client.on_message(client, None, SimpleNamespace(topic=TOPICS.command, payload=b"play"))
    assert recorder.commands == ["play"]


def test_publish_uses_qos_1(mqtt, client):
    mqtt.publish("t", "p", retain=True)
    assert client.called("publish") == [(("t", "p"), {"qos": 1, "retain": True})]


def test_unreachable_broker_is_logged(mqtt, client, caplog):
    client.on_connect_fail(client, None)
    assert "unreachable" in caplog.text

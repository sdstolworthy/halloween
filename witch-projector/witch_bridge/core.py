"""Maps Home Assistant commands to mpv requests and mpv events to Home Assistant state."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from typing import Any, Protocol

log = logging.getLogger(__name__)

PLAY = "play"
STOP = "stop"
ONLINE = "online"
OFFLINE = "offline"
IDLE_ACTIVE_OBSERVER_ID = 1


class Publisher(Protocol):
    def publish(self, topic: str, payload: str, retain: bool) -> None: ...


class Player(Protocol):
    def command(self, *args: Any) -> None: ...


@dataclass(frozen=True)
class Topics:
    node_id: str = "witch_projector"
    discovery_prefix: str = "homeassistant"

    @property
    def availability(self) -> str:
        return f"{self.node_id}/availability"

    @property
    def command(self) -> str:
        return f"{self.node_id}/command"

    @property
    def playing(self) -> str:
        return f"{self.node_id}/playing"


def discovery_messages(topics: Topics) -> list[tuple[str, str]]:
    device = {"identifiers": [topics.node_id], "name": "Witch projector", "model": "Raspberry Pi + mpv"}
    entities = [
        ("button", "play_witch", {"name": "Play witch", "command_topic": topics.command, "payload_press": PLAY,
                                  "icon": "mdi:play"}),
        ("button", "stop", {"name": "Stop", "command_topic": topics.command, "payload_press": STOP,
                            "icon": "mdi:stop"}),
        ("binary_sensor", "playing", {"name": "Playing", "state_topic": topics.playing, "device_class": "running"}),
    ]
    return [
        (
            f"{topics.discovery_prefix}/{component}/{topics.node_id}/{object_id}/config",
            json.dumps({
                "unique_id": f"{topics.node_id}_{object_id}",
                "availability_topic": topics.availability,
                "device": device,
                **fields,
            }),
        )
        for component, object_id, fields in entities
    ]


class Bridge:
    """Single-threaded core: every method is called from one event loop, so no locking is needed here."""

    def __init__(self, mqtt: Publisher, mpv: Player, topics: Topics, video_path: str) -> None:
        self._mqtt = mqtt
        self._mpv = mpv
        self._topics = topics
        self._video_path = video_path
        self._mpv_connected = False
        self._playing: bool | None = None

    def mqtt_connected(self) -> None:
        for topic, payload in discovery_messages(self._topics):
            self._mqtt.publish(topic, payload, retain=True)
        self._publish_availability()
        self._publish_playing()

    def mpv_connected(self) -> None:
        self._mpv_connected = True
        # mpv answers an observe with the current value straight away, so Playing is known without guessing.
        self._mpv.command("observe_property", IDLE_ACTIVE_OBSERVER_ID, "idle-active")
        self._publish_availability()

    def mpv_disconnected(self) -> None:
        self._mpv_connected = False
        self._playing = None
        self._publish_availability()

    def mpv_message(self, message: dict[str, Any]) -> None:
        if message.get("event") == "property-change" and message.get("name") == "idle-active":
            self._playing = not message.get("data")
            self._publish_playing()
        elif message.get("error") not in (None, "success"):
            log.warning("mpv rejected a command: %s", message)

    def command(self, payload: str) -> None:
        if payload == PLAY:
            self._mpv.command("loadfile", self._video_path, "replace")
        elif payload == STOP:
            self._mpv.command("stop")
        else:
            log.warning("Ignoring unknown command %r", payload)

    def _publish_availability(self) -> None:
        self._mqtt.publish(self._topics.availability, ONLINE if self._mpv_connected else OFFLINE, retain=True)

    def _publish_playing(self) -> None:
        if self._playing is not None:
            self._mqtt.publish(self._topics.playing, "ON" if self._playing else "OFF", retain=True)

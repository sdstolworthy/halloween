"""Maps Home Assistant commands to mpv requests and mpv events to Home Assistant state."""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from typing import Any, Callable, Mapping, Protocol, Sequence

log = logging.getLogger(__name__)

PLAY = "play"
PLAY_VIDEO_PREFIX = "play:"
STOP = "stop"
ONLINE = "online"
OFFLINE = "offline"
IDLE_ACTIVE_OBSERVER_ID = 1
PATH_OBSERVER_ID = 2


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


def discovery_messages(topics: Topics, video_names: Sequence[str]) -> list[tuple[str, str]]:
    device = {"identifiers": [topics.node_id], "name": "Witch projector", "model": "Raspberry Pi + mpv"}
    # play_witch keeps its original object id so the porch scare automation's entity survives the rename.
    entities = [
        ("button", "play_witch", {"name": "Random scare", "command_topic": topics.command, "payload_press": PLAY,
                                  "icon": "mdi:shuffle-variant"}),
        *(
            ("button", f"video_{name}", {"name": f"Play {name}", "command_topic": topics.command,
                                         "payload_press": f"{PLAY_VIDEO_PREFIX}{name}", "icon": "mdi:play"})
            for name in video_names
        ),
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

    def __init__(
        self,
        mqtt: Publisher,
        mpv: Player,
        topics: Topics,
        videos: Mapping[str, str],
        choose: Callable[[Sequence[str]], str],
        idle_video: str | None = None,
    ) -> None:
        self._mqtt = mqtt
        self._mpv = mpv
        self._topics = topics
        self._videos = dict(videos)
        self._choose = choose
        self._idle_video = idle_video
        self._idle_video_broken = False
        self._loaded: str | None = None
        self._last_video: str | None = None
        self._mpv_connected = False
        self._playing: bool | None = None

    def mqtt_connected(self) -> None:
        for topic, payload in discovery_messages(self._topics, list(self._videos)):
            self._mqtt.publish(topic, payload, retain=True)
        self._publish_availability()
        self._publish_playing()

    def mpv_connected(self) -> None:
        self._mpv_connected = True
        self._idle_video_broken = False
        # mpv answers an observe with the current value straight away, so Playing is known without guessing and an
        # idle mpv starts the happy loop at once.
        self._mpv.command("observe_property", IDLE_ACTIVE_OBSERVER_ID, "idle-active")
        self._mpv.command("observe_property", PATH_OBSERVER_ID, "path")
        self._publish_availability()

    def mpv_disconnected(self) -> None:
        self._mpv_connected = False
        self._playing = None
        self._publish_availability()

    def mpv_message(self, message: dict[str, Any]) -> None:
        event, name = message.get("event"), message.get("name")
        if event == "property-change" and name == "idle-active":
            if message.get("data"):
                self._loop_idle_video()
        elif event == "property-change" and name == "path":
            # The happy loop is always on screen between scares, so only a scare video counts as Playing.
            self._playing = message.get("data") in self._videos.values()
            self._publish_playing()
        elif event == "end-file":
            if message.get("reason") == "error" and self._loaded == self._idle_video:
                log.warning("Idle video %s failed to load; staying black between scares", self._idle_video)
                self._idle_video_broken = True
        elif message.get("error") not in (None, "success"):
            log.warning("mpv rejected a command: %s", message)

    def command(self, payload: str) -> None:
        if payload == PLAY:
            self._play_random()
        elif payload.startswith(PLAY_VIDEO_PREFIX):
            self._play(payload.removeprefix(PLAY_VIDEO_PREFIX))
        elif payload == STOP:
            self._mpv.command("stop")
        else:
            log.warning("Ignoring unknown command %r", payload)

    def _play_random(self) -> None:
        # Skipping the last clip means back-to-back scares never repeat, as long as there is more than one.
        candidates = [name for name in self._videos if name != self._last_video] or list(self._videos)
        if not candidates:
            log.warning("Random scare requested but no videos are configured")
            return
        self._play(self._choose(candidates))

    def _play(self, name: str) -> None:
        path = self._videos.get(name)
        if path is None:
            log.warning("Ignoring play for unknown video %r", name)
            return
        log.info("Playing %s", name)
        self._last_video = name
        self._loaded = path
        self._mpv.command("loadfile", path, "replace")

    def _loop_idle_video(self) -> None:
        if self._idle_video is None or self._idle_video_broken:
            return
        self._loaded = self._idle_video
        # A per-file option, so the loop ends when a scare replaces it instead of carrying over to the scare.
        self._mpv.command("loadfile", self._idle_video, "replace", -1, "loop-file=inf")

    def _publish_availability(self) -> None:
        self._mqtt.publish(self._topics.availability, ONLINE if self._mpv_connected else OFFLINE, retain=True)

    def _publish_playing(self) -> None:
        if self._playing is not None:
            self._mqtt.publish(self._topics.playing, "ON" if self._playing else "OFF", retain=True)

from __future__ import annotations

import posixpath
import re
from dataclasses import dataclass
from typing import Mapping

# Video names become MQTT topic segments, Home Assistant object ids and file names.
VIDEO_NAME = re.compile(r"[A-Za-z0-9_-]+")


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class Config:
    mqtt_host: str
    mqtt_port: int = 1883
    mqtt_username: str | None = None
    mqtt_password: str | None = None
    mpv_socket: str = "/run/witch/mpv.sock"
    videos_dir: str = "/opt/witch/videos"
    videos: tuple[str, ...] = ()
    discovery_prefix: str = "homeassistant"

    @classmethod
    def from_env(cls, env: Mapping[str, str]) -> Config:
        def value(name: str) -> str | None:
            return env.get(name) or None

        host = value("MQTT_HOST")
        if host is None:
            raise ConfigError("MQTT_HOST is required")
        port = value("MQTT_PORT") or str(cls.mqtt_port)
        if not port.isdigit():
            raise ConfigError(f"MQTT_PORT must be a number, got {port!r}")
        return cls(
            mqtt_host=host,
            mqtt_port=int(port),
            mqtt_username=value("MQTT_USERNAME"),
            mqtt_password=value("MQTT_PASSWORD"),
            mpv_socket=value("MPV_SOCKET") or cls.mpv_socket,
            videos_dir=value("WITCH_VIDEOS_DIR") or cls.videos_dir,
            videos=parse_videos(value("WITCH_VIDEOS") or ""),
            discovery_prefix=value("DISCOVERY_PREFIX") or cls.discovery_prefix,
        )

    def video_paths(self) -> dict[str, str]:
        return {name: posixpath.join(self.videos_dir, f"{name}.mp4") for name in self.videos}


def parse_videos(raw: str) -> tuple[str, ...]:
    if not raw.strip():
        return ()
    names = tuple(name.strip() for name in raw.split(","))
    for name in names:
        if not VIDEO_NAME.fullmatch(name):
            raise ConfigError(f"WITCH_VIDEOS names may only use letters, digits, _ and -, got {name!r}")
    if len(set(names)) != len(names):
        raise ConfigError(f"WITCH_VIDEOS has a duplicate name: {raw!r}")
    return names

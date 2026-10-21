from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping


class ConfigError(ValueError):
    pass


@dataclass(frozen=True)
class Config:
    mqtt_host: str
    mqtt_port: int = 1883
    mqtt_username: str | None = None
    mqtt_password: str | None = None
    mpv_socket: str = "/run/witch/mpv.sock"
    video_path: str = "/opt/witch/witch.mp4"
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
            video_path=value("WITCH_VIDEO") or cls.video_path,
            discovery_prefix=value("DISCOVERY_PREFIX") or cls.discovery_prefix,
        )

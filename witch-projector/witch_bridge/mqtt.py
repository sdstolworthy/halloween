"""Adapts a paho-mqtt v2 client to the bridge: last will, auth, reconnects and the command subscription."""

from __future__ import annotations

import logging
from typing import Any, Callable

from .core import OFFLINE, Topics

log = logging.getLogger(__name__)


class PahoMqtt:
    def __init__(
        self,
        client: Any,
        topics: Topics,
        on_connected: Callable[[], None],
        on_command: Callable[[str], None],
    ) -> None:
        self._client = client
        self._topics = topics
        self._on_connected = on_connected
        self._on_command = on_command
        client.on_connect = self._handle_connect
        client.on_connect_fail = self._handle_connect_fail
        client.on_disconnect = self._handle_disconnect
        client.on_message = self._handle_message

    def start(self, host: str, port: int, username: str | None, password: str | None) -> None:
        if username:
            self._client.username_pw_set(username, password)
        self._client.will_set(self._topics.availability, OFFLINE, qos=1, retain=True)
        self._client.reconnect_delay_set(min_delay=1, max_delay=30)
        self._client.connect_async(host, port, keepalive=30)
        # paho's loop thread retries the first connection and reconnects after any loss.
        self._client.loop_start()

    def publish(self, topic: str, payload: str, retain: bool) -> None:
        self._client.publish(topic, payload, qos=1, retain=retain)

    def _handle_connect(self, client: Any, userdata: Any, flags: Any, reason_code: Any, properties: Any) -> None:
        if reason_code.is_failure:
            log.error("MQTT connection refused: %s", reason_code)
            return
        log.info("Connected to MQTT")
        client.subscribe(self._topics.command, qos=1)
        self._on_connected()

    def _handle_connect_fail(self, client: Any, userdata: Any) -> None:
        log.warning("MQTT broker unreachable, retrying")

    def _handle_disconnect(self, client: Any, userdata: Any, flags: Any, reason_code: Any, properties: Any) -> None:
        log.warning("MQTT disconnected (%s), reconnecting", reason_code)

    def _handle_message(self, client: Any, userdata: Any, message: Any) -> None:
        self._on_command(message.payload.decode("utf-8", errors="replace"))

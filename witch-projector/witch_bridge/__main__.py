"""Wires the real MQTT client and mpv socket to the bridge and runs its event loop."""

from __future__ import annotations

import functools
import logging
import os
import queue
import random
import threading
import time
from typing import Any, Callable, Mapping

import paho.mqtt.client as paho

from .config import Config
from .core import Bridge, Topics
from .mpv import MpvIpc, open_unix_socket
from .mqtt import PahoMqtt


def main(env: Mapping[str, str]) -> None:
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(name)s: %(message)s")
    config = Config.from_env(env)
    topics = Topics(discovery_prefix=config.discovery_prefix)

    # paho and the mpv reader call back on their own threads; funnelling everything through one queue keeps the
    # Bridge single-threaded.
    events: queue.Queue[tuple[str, tuple[Any, ...]]] = queue.Queue()

    def post(handler: str) -> Callable[..., None]:
        return lambda *args: events.put((handler, args))

    mqtt = PahoMqtt(
        paho.Client(paho.CallbackAPIVersion.VERSION2, client_id="witch-bridge"),
        topics,
        on_connected=post("mqtt_connected"),
        on_command=post("command"),
    )
    mpv = MpvIpc(
        functools.partial(open_unix_socket, config.mpv_socket),
        on_connected=post("mpv_connected"),
        on_message=post("mpv_message"),
        on_disconnected=post("mpv_disconnected"),
        sleep=time.sleep,
    )
    bridge = Bridge(mqtt, mpv, topics, config.video_paths(), random.choice, config.idle_video_path())

    threading.Thread(target=mpv.run, args=(lambda: True,), name="mpv-ipc", daemon=True).start()
    mqtt.start(config.mqtt_host, config.mqtt_port, config.mqtt_username, config.mqtt_password)
    while True:
        handler, args = events.get()
        getattr(bridge, handler)(*args)


if __name__ == "__main__":
    main(os.environ)

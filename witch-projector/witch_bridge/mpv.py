"""mpv JSON IPC client that keeps reconnecting to mpv's Unix socket."""

from __future__ import annotations

import json
import logging
import socket
import threading
from typing import Any, Callable, Protocol

log = logging.getLogger(__name__)


class SocketLike(Protocol):
    def sendall(self, data: bytes) -> None: ...
    def recv(self, size: int) -> bytes: ...
    def close(self) -> None: ...


def encode_command(*args: Any) -> bytes:
    return json.dumps({"command": list(args)}).encode() + b"\n"


def decode_line(line: bytes) -> dict[str, Any] | None:
    try:
        message = json.loads(line)
    except ValueError:
        log.warning("Unparseable line from mpv: %r", line)
        return None
    return message if isinstance(message, dict) else None


def open_unix_socket(path: str) -> socket.socket:
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(path)
    except OSError:
        sock.close()
        raise
    return sock


class MpvIpc:
    def __init__(
        self,
        open_socket: Callable[[], SocketLike],
        on_connected: Callable[[], None],
        on_message: Callable[[dict[str, Any]], None],
        on_disconnected: Callable[[], None],
        sleep: Callable[[float], None],
        retry_seconds: float = 2.0,
    ) -> None:
        self._open_socket = open_socket
        self._on_connected = on_connected
        self._on_message = on_message
        self._on_disconnected = on_disconnected
        self._sleep = sleep
        self._retry_seconds = retry_seconds
        self._socket: SocketLike | None = None
        # Commands are sent from the event loop while run() reads on its own thread.
        self._lock = threading.Lock()

    def command(self, *args: Any) -> None:
        with self._lock:
            sock = self._socket
            if sock is None:
                log.warning("mpv not connected, dropping %s", args[0])
                return
            try:
                sock.sendall(encode_command(*args))
            except OSError as err:
                log.warning("Sending %s to mpv failed: %s", args[0], err)

    def run(self, keep_running: Callable[[], bool]) -> None:
        while keep_running():
            try:
                sock = self._open_socket()
            except OSError as err:
                log.debug("mpv socket not available: %s", err)
            else:
                self._serve(sock)
            self._sleep(self._retry_seconds)

    def _serve(self, sock: SocketLike) -> None:
        with self._lock:
            self._socket = sock
        log.info("Connected to mpv")
        self._on_connected()
        try:
            self._read_until_closed(sock)
        except OSError as err:
            log.warning("mpv connection lost: %s", err)
        finally:
            with self._lock:
                self._socket = None
            sock.close()
            log.info("Disconnected from mpv")
            self._on_disconnected()

    def _read_until_closed(self, sock: SocketLike) -> None:
        pending = b""
        while chunk := sock.recv(4096):
            *lines, pending = (pending + chunk).split(b"\n")
            for line in lines:
                if line.strip() and (message := decode_line(line)) is not None:
                    self._on_message(message)

import pytest

from witch_bridge.mpv import MpvIpc, decode_line, encode_command


class FakeSocket:
    def __init__(self, chunks=(), send_error=None, recv_error=None):
        self._chunks = list(chunks)
        self._send_error = send_error
        self._recv_error = recv_error
        self.sent = []
        self.closed = False

    def sendall(self, data):
        if self._send_error:
            raise self._send_error
        self.sent.append(data)

    def close(self):
        self.closed = True

    def recv(self, size):
        if self._chunks:
            return self._chunks.pop(0)
        if self._recv_error:
            raise self._recv_error
        return b""


class Recorder:
    def __init__(self):
        self.events = []
        self.sleeps = []


def make_ipc(open_results, recorder, on_connected=None):
    """open_results: sockets to return or exceptions to raise, one per connection attempt."""
    attempts = list(open_results)

    def open_socket():
        result = attempts.pop(0)
        if isinstance(result, Exception):
            raise result
        return result

    return MpvIpc(
        open_socket,
        on_connected=on_connected or (lambda: recorder.events.append("connected")),
        on_message=lambda message: recorder.events.append(message),
        on_disconnected=lambda: recorder.events.append("disconnected"),
        sleep=recorder.sleeps.append,
        retry_seconds=2.0,
    )


def runs(count):
    remaining = iter([True] * count)
    return lambda: next(remaining, False)


def test_encode_command_is_one_json_line():
    assert encode_command("loadfile", "/v.mp4", "replace") == b'{"command": ["loadfile", "/v.mp4", "replace"]}\n'


@pytest.mark.parametrize(
    "line, expected",
    [
        (b'{"event": "idle"}', {"event": "idle"}),
        (b"not json", None),
        (b"[1, 2]", None),
    ],
)
def test_decode_line(line, expected):
    assert decode_line(line) == expected


def test_messages_split_across_chunks_are_reassembled():
    recorder = Recorder()
    sock = FakeSocket([b'{"event": "st', b'art-file"}\n{"event": "idle"}\n'])
    make_ipc([sock], recorder).run(runs(1))
    assert recorder.events == ["connected", {"event": "start-file"}, {"event": "idle"}, "disconnected"]


def test_failed_open_waits_and_retries():
    recorder = Recorder()
    make_ipc([FileNotFoundError(), ConnectionRefusedError(), FakeSocket()], recorder).run(runs(3))
    assert (recorder.events, recorder.sleeps) == (["connected", "disconnected"], [2.0, 2.0, 2.0])


def test_read_error_reports_disconnect_and_keeps_running():
    recorder = Recorder()
    lost = FakeSocket(recv_error=ConnectionResetError())
    make_ipc([lost, FakeSocket()], recorder).run(runs(2))
    assert recorder.events == ["connected", "disconnected", "connected", "disconnected"]


def test_socket_is_closed_after_disconnect():
    sock = FakeSocket()
    make_ipc([sock], Recorder()).run(runs(1))
    assert sock.closed


def test_command_while_connected_is_sent():
    sock = FakeSocket()
    recorder = Recorder()
    ipc = None

    def send_on_connect():
        ipc.command("stop")

    ipc = make_ipc([sock], recorder, on_connected=send_on_connect)
    ipc.run(runs(1))
    assert sock.sent == [b'{"command": ["stop"]}\n']


def test_command_while_disconnected_is_dropped_with_a_warning(caplog):
    make_ipc([], Recorder()).command("stop")
    assert "dropping stop" in caplog.text


def test_send_failure_is_logged_not_raised(caplog):
    sock = FakeSocket(send_error=BrokenPipeError())
    ipc = None

    def send_on_connect():
        ipc.command("stop")

    ipc = make_ipc([sock], Recorder(), on_connected=send_on_connect)
    ipc.run(runs(1))
    assert "Sending stop to mpv failed" in caplog.text

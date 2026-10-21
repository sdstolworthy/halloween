from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class FakePublisher:
    published: list[tuple[str, str, bool]] = field(default_factory=list)

    def publish(self, topic: str, payload: str, retain: bool) -> None:
        self.published.append((topic, payload, retain))

    def payloads_for(self, topic: str) -> list[str]:
        return [payload for published_topic, payload, _ in self.published if published_topic == topic]


@dataclass
class FakePlayer:
    commands: list[tuple[Any, ...]] = field(default_factory=list)

    def command(self, *args: Any) -> None:
        self.commands.append(args)

from dataclasses import dataclass
from enum import StrEnum


class Task(StrEnum):
    FOLLOW = "follow"
    COLLECT = "collect"
    ESCAPE = "escape"


@dataclass(frozen=True)
class RewardConfig:
    version: str = "v1"
    resource: float = 5.0
    follow_tick: float = 1.0
    damage: float = -20.0
    step: float = -0.1
    invalid: float = -0.5
    success: float = 20.0

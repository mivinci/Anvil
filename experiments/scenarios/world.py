"""A deterministic toy environment with local observations and discrete time."""
from collections import deque
from copy import deepcopy
from dataclasses import dataclass

from .maps import HEIGHT, WIDTH, make_map
from .tasks import RewardConfig, Task

Point = tuple[int, int]
MOVES = {"north": (0, -1), "south": (0, 1), "west": (-1, 0), "east": (1, 0)}
ACTIONS = (*MOVES, "wait", "collect")
RADIUS = 4


def distance(a: Point, b: Point) -> int:
    return abs(a[0] - b[0]) + abs(a[1] - b[1])


@dataclass
class Observation:
    position: Point
    cells: dict[Point, str]
    player: Point | None
    creeper: Point | None
    fuse: int | None
    collected: int
    health: int
    tick: int
    task: Task
    previous_action: str
    stationary_steps: int


class World:
    def __init__(self, seed: int, task: Task, rewards: RewardConfig = RewardConfig(), task_action_mask: bool = False):
        self.task_action_mask = task_action_mask
        self.seed = seed
        self.task = task
        self.rewards = rewards
        self.walls, floor = make_map(seed)
        self.position, self.player, self.creeper = floor[:3]
        if task == Task.ESCAPE:
            self.creeper = min(
                (point for point in floor if point != self.position),
                key=lambda point: abs(distance(point, self.position) - 3),
            )
        self.resources = {point: 1 for point in floor[3:9]}
        self.fuse = 8
        self.health = 1
        self.tick = 0
        self.collected = 0
        self.goal_quantity = 3
        self.follow_ticks = 0
        self.previous_action = "wait"
        self.stationary_steps = 0
        self.done = False
        self.reason = None
        self.total_reward = 0.0
        # Exogenous motion depends only on seed and time, not bot actions.
        self.player_route = floor[9:15]
        self.visited = {self.position}
        self.recent_positions = [self.position]
        self.last_collection_tick = 0
        self.player_last_seen = None
        self.player_previous_seen = None
        self._remember_player()

    def clone(self):
        return deepcopy(self)

    def observe(self) -> Observation:
        x, y = self.position
        cells = {}
        for px in range(max(0, x - RADIUS), min(WIDTH, x + RADIUS + 1)):
            for py in range(max(0, y - RADIUS), min(HEIGHT, y + RADIUS + 1)):
                point = (px, py)
                cells[point] = (
                    "wall" if point in self.walls else
                    "resource" if self.resources.get(point, 0) else "floor"
                )
        visible = lambda point: point if point in cells else None
        creeper = visible(self.creeper) if self.fuse > 0 else None
        return Observation(
            self.position, cells, visible(self.player), creeper,
            self.fuse if creeper else None, self.collected, self.health,
            self.tick, self.task, self.previous_action, self.stationary_steps,
        )

    def candidates(self) -> list[str]:
        observation = self.observe()
        x, y = self.position
        actions = [
            name for name, (dx, dy) in MOVES.items()
            if observation.cells.get((x + dx, y + dy)) in ("floor", "resource")
        ]
        actions.append("wait")
        if (not self.task_action_mask or self.task == Task.COLLECT) and any(distance(self.position, point) <= 1 and count > 0
               for point, count in self.resources.items()):
            actions.append("collect")
        return actions

    def step(self, action: str) -> float:
        if self.done:
            raise ValueError("episode already terminated")
        if action not in ACTIONS:
            raise ValueError(f"unknown action: {action}")
        reward = self.rewards.step
        before = self.position
        if action not in self.candidates():
            reward += self.rewards.invalid
        elif action in MOVES:
            dx, dy = MOVES[action]
            self.position = (before[0] + dx, before[1] + dy)
        elif action == "collect":
            target = next(
                point for point, count in sorted(self.resources.items())
                if count > 0 and distance(self.position, point) <= 1
            )
            self.resources[target] -= 1
            self.collected += 1
            self.last_collection_tick = self.tick + 1
            if self.task == Task.COLLECT:
                reward += self.rewards.resource
        self.stationary_steps = self.stationary_steps + 1 if before == self.position else 0
        self.previous_action = action
        self.visited.add(self.position)
        self.recent_positions = (self.recent_positions + [self.position])[-8:]
        self.tick += 1
        self._move_player()
        self._remember_player()
        if self.fuse > 0:
            self.fuse -= 1
            if self.fuse == 0 and distance(self.position, self.creeper) <= 2:
                self.health = 0
                reward += self.rewards.damage
        if self.task == Task.FOLLOW and distance(self.position, self.player) <= 2:
            self.follow_ticks += 1
            reward += self.rewards.follow_tick
        success = (
            self.task == Task.COLLECT and self.collected >= self.goal_quantity
            or self.task == Task.FOLLOW and self.follow_ticks >= 12
            or self.task == Task.ESCAPE and self.fuse == 0 and self.health > 0
        )
        if self.health == 0:
            self.reason = "death"
        elif success:
            self.reason = "success"
            reward += self.rewards.success
        elif self.tick >= 80:
            self.reason = "timeout"
        self.done = self.reason is not None
        self.total_reward += reward
        return reward

    def _remember_player(self):
        if (abs(self.position[0] - self.player[0]) <= RADIUS
                and abs(self.position[1] - self.player[1]) <= RADIUS):
            self.player_previous_seen = self.player_last_seen
            self.player_last_seen = (self.player, self.tick)

    def _move_player(self):
        target = self.player_route[(self.tick // 10) % len(self.player_route)]
        path = first_step(self.player, {target}, {
            (x, y) for x in range(WIDTH) for y in range(HEIGHT)
            if (x, y) not in self.walls
        })
        if path in MOVES:
            dx, dy = MOVES[path]
            self.player = (self.player[0] + dx, self.player[1] + dy)


def first_step(start: Point, goals: set[Point], floor: set[Point]) -> str:
    if start in goals:
        return "wait"
    queue = deque([(start, "wait")])
    visited = {start}
    while queue:
        point, first = queue.popleft()
        for name, (dx, dy) in MOVES.items():
            neighbor = (point[0] + dx, point[1] + dy)
            if neighbor not in floor or neighbor in visited:
                continue
            direction = name if point == start else first
            if neighbor in goals:
                return direction
            visited.add(neighbor)
            queue.append((neighbor, direction))
    return "wait"


def baseline(world: World) -> str:
    """Local-information teacher v1. It never reads hidden walls or entities."""
    observation = world.observe()
    candidates = world.candidates()
    if observation.creeper is not None:
        def safety(action):
            dx, dy = MOVES.get(action, (0, 0))
            point = (observation.position[0] + dx, observation.position[1] + dy)
            return distance(point, observation.creeper)
        return max(candidates, key=safety)
    if observation.task == Task.COLLECT and "collect" in candidates:
        return "collect"
    floor = {point for point, kind in observation.cells.items() if kind != "wall"}
    if observation.task == Task.FOLLOW and observation.player is not None:
        goals = {point for point in floor if distance(point, observation.player) <= 2}
    elif observation.task == Task.COLLECT:
        goals = {point for point, kind in observation.cells.items() if kind == "resource"}
    else:
        goals = set()
    if not goals:
        # Frontier exploration uses remembered visited cells, not hidden state.
        goals = floor - world.visited
    return first_step(observation.position, goals, floor)

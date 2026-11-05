"""Run from experiments with uv run python -m dataset.generate."""
import argparse
import hashlib
import json
import random
from dataclasses import asdict
from pathlib import Path

from scenarios.tasks import Task
from scenarios.world import ACTIONS, MOVES, RADIUS, World, baseline

SPLITS = {"train": 0, "validation": 1_000_000, "test": 2_000_000}


def relative(point, origin):
    return [point[0] - origin[0], point[1] - origin[1]] if point else [0, 0]


def features(world: World, action: str, version: int = 1) -> dict:
    observation = world.observe()
    dx, dy = MOVES.get(action, (0, 0))
    values = {
        "task": list(Task).index(world.task),
        "action": ACTIONS.index(action),
        "move_x": dx, "move_y": dy,
        "health": observation.health,
        "collected": observation.collected,
        "quantity": world.goal_quantity,
        "player_known": int(observation.player is not None),
        "creeper_known": int(observation.creeper is not None),
        "fuse": observation.fuse if observation.fuse is not None else -1,
        "previous_action": ACTIONS.index(observation.previous_action),
        "stationary_steps": observation.stationary_steps,
    }
    for name, point in [("player", observation.player), ("creeper", observation.creeper)]:
        values[f"{name}_x"], values[f"{name}_y"] = relative(point, observation.position)
    x, y = observation.position
    encoding = {None: -1, "floor": 0, "wall": 1, "resource": 2}
    for ox in range(-RADIUS, RADIUS + 1):
        for oy in range(-RADIUS, RADIUS + 1):
            point = (x + ox, y + oy)
            values[f"cell_{ox}_{oy}"] = encoding[observation.cells.get(point)]
            values[f"visited_{ox}_{oy}"] = int(point in world.visited)
    if version in (2, 3, 4, 5):
        from .action_features import candidate_features
        values.update(candidate_features(world, action))
        if version in (3, 4, 5):
            from .action_features import threat_features
            values.update(threat_features(world, action))
        if version in (4, 5):
            point = (x + dx, y + dy)
            previous_move = MOVES.get(world.previous_action, (0, 0))
            values.update({
                "reverses_previous_move": int((dx, dy) != (0, 0) and (dx, dy) == (-previous_move[0], -previous_move[1])),
                "recent_candidate_visits": world.recent_positions.count(point),
                "recent_unique_positions": len(set(world.recent_positions)),
                "steps_since_collection": world.tick - world.last_collection_tick,
            })
        if version == 5:
            from .action_features import follow_memory_features
            values.update(follow_memory_features(world, action))
    elif version != 1:
        raise ValueError("unsupported features version")
    return values


def candidate_scores(world: World, horizon: int, label_version: int = 1) -> list[tuple[str, float]]:
    if label_version not in (1, 2):
        raise ValueError("unsupported label version")
    gamma = 0.95 if label_version == 2 else 1.0
    result = []
    for action in world.candidates():
        branch = world.clone()
        reward = branch.step(action)
        for step in range(1, horizon):
            if branch.done:
                break
            reward += gamma ** step * branch.step(baseline(branch))
        result.append((action, reward))
    return result


def map_id(world: World) -> str:
    return hashlib.sha256(json.dumps(sorted(world.walls)).encode()).hexdigest()


def generate(output: Path, episodes: int, horizon: int, feature_version: int = 1, actor=None, threat_starts: int = 0, label_version: int = 1, task_action_mask: bool = False, follow_starts: int = 0):
    if episodes < 1 or episodes >= 1_000_000 or horizon < 1 or threat_starts < 0 or follow_starts < 0:
        raise ValueError("episodes must be 1..999999 and horizon must be positive")
    output.mkdir(parents=True, exist_ok=False)
    manifest = {
        "schema_version": 1, "features_version": feature_version, "teacher": "local-baseline-v1",
        "horizon": horizon, "episodes_per_split": episodes, "seed_offsets": SPLITS,
        "simulation_version": 1, "splits": {},
    }
    manifest["task_action_mask"] = task_action_mask
    manifest["label_version"] = label_version
    manifest["discount"] = 0.95 if label_version == 2 else 1.0
    manifest["training_actor"] = actor.metadata if actor is not None else None
    manifest["threat_starts_per_training_map"] = threat_starts
    manifest["follow_starts_per_training_map"] = follow_starts
    seen_maps = set()
    for split, offset in SPLITS.items():
        maps = []
        count = 0
        terminations = {}
        with (output / f"{split}.jsonl").open("w") as stream:
            for episode in range(episodes):
                task = list(Task)[episode % len(Task)]
                world = World(offset + episode, task, task_action_mask=task_action_mask)
                identity = map_id(world)
                if identity in seen_maps:
                    raise ValueError("duplicate map across episodes; use another seed configuration")
                seen_maps.add(identity)
                maps.append(identity)
                policies = [("baseline", baseline)]
                if split == "train" and actor is not None:
                    policies.append(("model", actor.choose))
                if split == "train":
                    for trial in range(threat_starts):
                        policies.append((f"threat-{trial}", actor.choose if actor else baseline))
                if split == "train" and task == Task.FOLLOW:
                    for trial in range(follow_starts):
                        policies.append((f"follow-{trial}-baseline", baseline))
                        if actor is not None:
                            policies.append((f"follow-{trial}-model", actor.choose))
                for source, choose in policies:
                    world = World(offset + episode, task, task_action_mask=task_action_mask)
                    if source.startswith("threat-"):
                        trial = int(source.split("-")[1])
                        rng = random.Random((offset + episode) * 1009 + trial)
                        position = world.position
                        nearby = [
                            point for point, kind in world.observe().cells.items()
                            if kind != "wall" and 1 <= abs(point[0] - position[0]) + abs(point[1] - position[1]) <= 4
                        ]
                        world.creeper = rng.choice(sorted(nearby))
                        world.fuse = rng.randint(1, 4)
                    if source.startswith("follow-"):
                        from scenarios.follow import PROFILES, configure_follow
                        trial = int(source.split("-")[1])
                        configure_follow(world, PROFILES[trial % len(PROFILES)], trial)
                    initial_state = {"position": world.position, "creeper": world.creeper, "fuse": world.fuse, "player": world.player, "player_route": world.player_route}
                    episode_id = f"{split}-{episode}-{source}"
                    while not world.done:
                        scores = candidate_scores(world, horizon, label_version)
                        # Integer relevance preserves ties within a decision group.
                        levels = sorted({round(score, 6) for _, score in scores})
                        executed = choose(world)
                        observation = world.observe()
                        for candidate_id, (action, score) in enumerate(scores):
                            row = {
                                "schema_version": 1, "features_version": feature_version,
                                "scenario_id": task.value, "map_id": identity,
                                "episode_id": episode_id, "decision_id": world.tick,
                                "tick": world.tick, "task": task.value, "initial_state": initial_state,
                                "candidate_id": candidate_id, "action": action,
                                "valid": True, "features": features(world, action, feature_version),
                                "relevance": levels.index(round(score, 6)),
                                "rollout_return": score, "label_source": "branch_simulation",
                                "executed": action == executed, "outcome": None,
                                "observation": {
                                    "position": observation.position,
                                    "player": observation.player,
                                    "creeper": observation.creeper,
                                },
                            }
                            # Log actual one-step outcomes separately from simulated labels.
                            if action == executed:
                                actual = world.clone()
                                immediate_reward = actual.step(action)
                                row["outcome"] = {
                                    "reward": immediate_reward, "reason": actual.reason,
                                    "health": actual.health, "collected": actual.collected,
                                }
                            stream.write(json.dumps(row, sort_keys=True) + "\n")
                            count += 1
                        world.step(executed)
                    manifest.setdefault("rewards", asdict(world.rewards))
                    terminations[world.reason] = terminations.get(world.reason, 0) + 1
        manifest["splits"][split] = {"maps": maps, "rows": count, "terminations": terminations}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("outputs/dataset-v1"))
    parser.add_argument("--episodes", type=int, default=30, help="episodes per split")
    parser.add_argument("--horizon", type=int, default=8, help="steps including candidate action")
    parser.add_argument("--features-version", type=int, choices=[1, 2, 3, 4, 5], default=1)
    parser.add_argument("--actor", type=Path, help="model whose training-map states are additionally labeled")
    parser.add_argument("--threat-starts", type=int, default=0)
    parser.add_argument("--label-version", type=int, choices=[1, 2], default=1)
    parser.add_argument("--task-action-mask", action="store_true")
    parser.add_argument("--follow-starts", type=int, default=0)
    args = parser.parse_args()
    actor = None
    if args.actor:
        from evaluation.policy import RankingPolicy
        actor = RankingPolicy(args.actor)
    manifest = generate(args.output, args.episodes, args.horizon, args.features_version, actor, args.threat_starts, args.label_version, args.task_action_mask, args.follow_starts)
    print(json.dumps({name: data["rows"] for name, data in manifest["splits"].items()}))


if __name__ == "__main__":
    main()

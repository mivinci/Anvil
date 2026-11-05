"""Replay failures with candidate scores, local observations and rollout labels."""
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np

from dataset.generate import candidate_scores, features
from evaluation.policy import RankingPolicy
from scenarios.tasks import Task
from scenarios.world import World


def diagnose(model: Path, output: Path, report: Path | None = None, task: Task | None = None):
    policy = RankingPolicy(model)
    manifest = policy.metadata["dataset_manifest"]
    failures = []
    action_mask = manifest.get("task_action_mask", False)
    if report:
        recorded = json.loads(report.read_text())
        action_mask = recorded.get("task_action_mask", action_mask)
        model_hash = hashlib.sha256((model / "model.ubj").read_bytes()).hexdigest()
        if recorded["model_sha256"] != model_hash:
            raise ValueError("report model does not match")
        cases = [
            row for row in recorded["episodes"]
            if row["policy"] == "xgboost" and row["reason"] != "success"
            and (task is None or row["task"] == task.value)
        ]
    else:
        cases = [
            {"seed": manifest["seed_offsets"]["validation"] + index,
             "task": list(Task)[index % len(Task)].value, "initial_fuse": 8}
            for index in range(manifest["episodes_per_split"])
        ]
    for case in cases:
        world = World(case["seed"], Task(case["task"]), task_action_mask=action_mask)
        world.fuse = case["initial_fuse"]
        decisions = []
        while not world.done:
            labels = candidate_scores(world, manifest["horizon"], manifest.get("label_version", 1))
            actions = [action for action, _ in labels]
            rows = [features(world, action, policy.metadata["features_version"]) for action in actions]
            matrix = np.asarray([[row[name] for name in policy.names] for row in rows], dtype=np.float32)
            scores = policy.model.predict(matrix)
            chosen = actions[int(np.argmax(scores))]
            observation = world.observe()
            decisions.append({
                "tick": world.tick, "position": world.position,
                "visible_creeper": observation.creeper, "visible_fuse": observation.fuse,
                "visible_player": observation.player,
                "previous_action": observation.previous_action,
                "follow_ticks": world.follow_ticks,
                "chosen": chosen,
                "candidates": [
                    {"action": action, "model_score": float(score), "rollout_return": label,
                     "features": features(world, action, policy.metadata["features_version"])}
                    for (action, label), score in zip(labels, scores)
                ],
            })
            world.step(chosen)
        if report and [decision["chosen"] for decision in decisions] != case["actions"]:
            raise ValueError("replayed actions differ from report")
        if world.reason != "success" and (task is None or world.task == task):
            failures.append({"seed": world.seed, "task": world.task.value,
                             "reason": world.reason, "decisions": decisions})
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x") as stream:
        json.dump(failures, stream, indent=2)
    for failure in failures:
        print(failure["seed"], failure["task"])
        for decision in failure["decisions"][-3:]:
            print(json.dumps({key: value for key, value in decision.items() if key != "candidates"}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=Path, default=Path("outputs/xgboost-v2-augmented"))
    parser.add_argument("--output", type=Path, default=Path("outputs/diagnosis-v2.json"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--task", type=Task, choices=list(Task))
    args = parser.parse_args()
    diagnose(args.model, args.output, args.report, args.task)

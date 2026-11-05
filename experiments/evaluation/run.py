"""Closed-loop evaluation on the held-out test split's map seeds."""
import argparse
import hashlib
import json
from pathlib import Path

from dataset.generate import map_id
from evaluation.policy import load_policy, policy_name, model_digest
from scenarios.tasks import Task
from scenarios.world import World, baseline


def episode(world, choose):
    reset = getattr(getattr(choose, "__self__", None), "reset", None)
    if reset is not None:
        reset()
    previous = None
    switches = 0
    actions = []
    while not world.done:
        action = choose(world)
        switches += previous is not None and action != previous
        world.step(action)
        actions.append(action)
        previous = action
    return {
        "task": world.task.value, "seed": world.seed, "map_id": map_id(world),
        "reason": world.reason, "steps": world.tick,
        "death": world.health == 0, "reward": world.total_reward,
        "switches": switches, "actions": actions,
    }


def evaluate(model: Path, output: Path, split: str = "test"):
    policy = load_policy(model)
    manifest = policy.metadata["dataset_manifest"]
    if policy.metadata["simulation_version"] != 1:
        raise ValueError("unsupported simulation version")
    results = []
    for index in range(manifest["episodes_per_split"]):
        task = list(Task)[index % len(Task)]
        seed = manifest["seed_offsets"][split] + index
        for name, choose in [("baseline", baseline), (policy_name(policy), policy.choose)]:
            world = World(seed, task, task_action_mask=manifest.get("task_action_mask", False))
            if map_id(world) != manifest["splits"][split]["maps"][index]:
                raise ValueError("test map changed since dataset generation")
            results.append({"policy": name, **episode(world, choose)})
    summary = []
    for name in ["baseline", policy_name(policy)]:
        for task in Task:
            group = [row for row in results if row["policy"] == name and row["task"] == task]
            if not group:
                continue
            summary.append({
                "policy": name, "task": task.value, "episodes": len(group),
                "success_rate": sum(row["reason"] == "success" for row in group) / len(group),
                "death_rate": sum(row["death"] for row in group) / len(group),
                "mean_steps": sum(row["steps"] for row in group) / len(group),
                "mean_switches": sum(row["switches"] for row in group) / len(group),
            })
    output.mkdir(parents=True, exist_ok=False)
    report = {
        "model_sha256": model_digest(model),
        "split": split, "summary": summary, "episodes": results,
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=Path, default=Path("outputs/xgboost-v1"))
    parser.add_argument("--output", type=Path, default=Path("outputs/evaluation-v1"))
    parser.add_argument("--split", choices=["validation", "test"], default="test")
    args = parser.parse_args()
    evaluate(args.model, args.output, args.split)

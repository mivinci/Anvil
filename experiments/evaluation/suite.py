"""Freeze a larger unseen-map suite before running the fixed model."""
import argparse
import hashlib
import json
from pathlib import Path

from dataset.generate import map_id
from evaluation.policy import load_policy, policy_name, model_digest
from evaluation.run import episode
from scenarios.tasks import Task
from scenarios.world import World, baseline

FUSES = (2, 4, 8, 12)


def build_cases(manifest: dict, episodes: int, seed_start: int):
    if episodes < 12 or episodes % 12:
        raise ValueError("episodes must be a positive multiple of 12")
    used = {identity for split in manifest["splits"].values() for identity in split["maps"]}
    cases = []
    for index in range(episodes):
        task = list(Task)[index % len(Task)]
        world = World(seed_start + index, task)
        identity = map_id(world)
        if identity in used:
            raise ValueError("suite map overlaps an existing map")
        used.add(identity)
        cases.append({
            "seed": world.seed, "task": task.value, "map_id": identity,
            "initial_fuse": FUSES[(index // len(Task)) % len(FUSES)],
        })
    return cases


def summarize(rows):
    count = len(rows)
    return {
        "episodes": count,
        "successes": sum(row["reason"] == "success" for row in rows),
        "deaths": sum(row["death"] for row in rows),
        "timeouts": sum(row["reason"] == "timeout" for row in rows),
        "mean_steps": sum(row["steps"] for row in rows) / count,
        "mean_switches": sum(row["switches"] for row in rows) / count,
    }


def evaluate_suite(model: Path, output: Path, episodes: int, seed_start: int, mask_task_actions: bool = False):
    policy = load_policy(model)
    if policy.metadata["simulation_version"] != 1:
        raise ValueError("unsupported simulation version")
    effective_mask = mask_task_actions or policy.metadata["dataset_manifest"].get("task_action_mask", False)
    cases = build_cases(policy.metadata["dataset_manifest"], episodes, seed_start)
    model_hash = model_digest(model)
    plan = {
        "suite_version": 1, "simulation_version": 1,
        "model_sha256": model_hash,
        "features_version": policy.metadata["features_version"],
        "task_action_mask": effective_mask,
        "cases": cases,
    }
    output.mkdir(parents=True, exist_ok=False)
    plan_bytes = (json.dumps(plan, indent=2) + "\n").encode()
    (output / "plan.json").write_bytes(plan_bytes)
    results = []
    for case in cases:
        for name, choose in [("baseline", baseline), (policy_name(policy), policy.choose)]:
            world = World(case["seed"], Task(case["task"]), task_action_mask=effective_mask)
            world.fuse = case["initial_fuse"]
            results.append({"policy": name, "initial_fuse": world.fuse, **episode(world, choose)})
    if model_digest(model) != model_hash:
        raise ValueError("model changed during evaluation")
    summary = []
    detailed = []
    for name in ["baseline", policy_name(policy)]:
        for task in Task:
            rows = [row for row in results if row["policy"] == name and row["task"] == task]
            summary.append({"policy": name, "task": task.value, **summarize(rows)})
            for fuse in FUSES:
                subset = [row for row in rows if row["initial_fuse"] == fuse]
                detailed.append({"policy": name, "task": task.value, "initial_fuse": fuse, **summarize(subset)})
    report = {
        "plan_sha256": hashlib.sha256(plan_bytes).hexdigest(),
        "task_action_mask": effective_mask,
        "model_sha256": model_hash,
        "summary": summary, "by_fuse": detailed, "episodes": results,
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=Path, default=Path("outputs/xgboost-v3-threats"))
    parser.add_argument("--output", type=Path, default=Path("outputs/blind-suite-v1"))
    parser.add_argument("--episodes", type=int, default=300)
    parser.add_argument("--seed-start", type=int, default=3_000_000)
    parser.add_argument("--mask-task-actions", action="store_true")
    args = parser.parse_args()
    evaluate_suite(args.model, args.output, args.episodes, args.seed_start, args.mask_task_actions)

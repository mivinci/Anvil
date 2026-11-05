"""Validation-only follow curriculum evaluation, with separate profile metrics."""
import argparse
import hashlib
import json
from pathlib import Path

from evaluation.policy import load_policy, policy_name, model_digest
from evaluation.run import episode
from evaluation.suite import summarize
from scenarios.follow import PROFILES, configure_follow
from scenarios.tasks import Task
from scenarios.world import World


def evaluate_follow(model: Path, output: Path, mask_task_actions: bool = False):
    from scenarios.world import baseline

    policy = load_policy(model)
    manifest = policy.metadata["dataset_manifest"]
    results = []
    for index in range(manifest["episodes_per_split"]):
        seed = manifest["seed_offsets"]["validation"] + index
        for profile in PROFILES:
            for name, choose in [("baseline", baseline), (policy_name(policy), policy.choose)]:
                world = World(seed, Task.FOLLOW, task_action_mask=mask_task_actions or manifest.get("task_action_mask", False))
                configure_follow(world, profile)
                results.append({"policy": name, "profile": profile, **episode(world, choose)})
    summary = [
        {"policy": name, "profile": profile, **summarize([
            row for row in results if row["policy"] == name and row["profile"] == profile
        ])}
        for name in ["baseline", policy_name(policy)] for profile in PROFILES
    ]
    output.mkdir(parents=True, exist_ok=False)
    (output / "report.json").write_text(json.dumps({
        "model_sha256": model_digest(model),
        "split": "validation", "task_action_mask": mask_task_actions or manifest.get("task_action_mask", False), "summary": summary, "episodes": results,
    }, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mask-task-actions", action="store_true")
    args = parser.parse_args()
    evaluate_follow(args.model, args.output, args.mask_task_actions)

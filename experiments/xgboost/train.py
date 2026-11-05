"""Run from experiments: uv run python xgboost/train.py."""
import argparse
import hashlib
import json
import sys

from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import numpy as np
import xgboost as xgb

from dataset.load import load_split


def train(dataset: Path, output: Path):
    manifest = json.loads((dataset / "manifest.json").read_text())
    partitions = [set(info["maps"]) for info in manifest["splits"].values()]
    for index, partition in enumerate(partitions):
        if any(partition & other for other in partitions[index + 1:]):
            raise ValueError("map leakage across dataset splits")
    train_x, train_y, train_groups, names = load_split(dataset / "train.jsonl")
    val_x, val_y, val_groups, _ = load_split(dataset / "validation.jsonl", names)
    model = xgb.XGBRanker(
        objective="rank:pairwise", n_estimators=200, max_depth=4,
        learning_rate=0.05, n_jobs=1, random_state=0,
        eval_metric="ndcg", early_stopping_rounds=20,
    )
    model.fit(
        train_x, train_y, group=train_groups,
        eval_set=[(val_x, val_y)], eval_group=[val_groups], verbose=False,
    )
    scores = model.predict(val_x)
    informative = correct = start = 0
    for size in val_groups:
        labels = val_y[start:start + size]
        if np.ptp(labels) > 0:
            informative += 1
            correct += labels[np.argmax(scores[start:start + size])] == labels.max()
        start += size
    output.mkdir(parents=True, exist_ok=False)
    model.save_model(output / "model.ubj")
    metadata = {
        "schema_version": 1, "features_version": manifest["features_version"],
        "feature_names": names, "xgboost_version": xgb.__version__,
        "simulation_version": manifest["simulation_version"],
        "dataset_manifest": manifest,
        "train_sha256": hashlib.sha256((dataset / "train.jsonl").read_bytes()).hexdigest(),
        "validation_sha256": hashlib.sha256((dataset / "validation.jsonl").read_bytes()).hexdigest(),
        "parameters": model.get_params(),
        "best_iteration": model.best_iteration,
        "validation_informative_groups": informative,
        "validation_top_choice_accuracy": int(correct) / informative if informative else None,
    }
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps({key: metadata[key] for key in [
        "best_iteration", "validation_informative_groups", "validation_top_choice_accuracy",
    ]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, default=Path("outputs/dataset-v1"))
    parser.add_argument("--output", type=Path, default=Path("outputs/xgboost-v1"))
    args = parser.parse_args()
    train(args.dataset, args.output)

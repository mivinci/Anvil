"""Run from experiments: uv run python -m transformer.train."""
import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path

import numpy as np
import torch
from torch.utils.data import DataLoader, TensorDataset

from dataset.load import load_split
from .data import samples
from .model import CandidateTransformer, pairwise_loss


def validate(model, data):
    model.eval()
    total_loss = correct = count = 0
    with torch.no_grad():
        for past, current, history_valid, valid, labels in DataLoader(TensorDataset(*data), batch_size=128):
            scores = model(past, current, history_valid, valid)
            informative = labels.masked_fill(~valid, 0).max(dim=1).values > labels.masked_fill(~valid, 1e9).min(dim=1).values
            groups = int(informative.sum())
            if not groups:
                continue
            total_loss += float(pairwise_loss(scores, labels, valid)) * groups
            selected = labels.gather(1, scores.argmax(dim=1, keepdim=True)).squeeze(1)
            correct += int(((selected == labels.max(dim=1).values) & informative).sum())
            count += groups
    if not count:
        raise ValueError("validation has no informative groups")
    return total_loss / count, correct / count


def train(dataset: Path, output: Path, epochs: int, seed: int):
    if epochs < 1:
        raise ValueError("epochs must be positive")
    torch.set_num_threads(1)
    torch.manual_seed(seed)
    manifest = json.loads((dataset / "manifest.json").read_text())
    maps = [set(split["maps"]) for split in manifest["splits"].values()]
    if any(group & other for index, group in enumerate(maps) for other in maps[index + 1:]):
        raise ValueError("map leakage")
    x, _, _, names = load_split(dataset / "train.jsonl")
    mean, scale = x.mean(axis=0), x.std(axis=0).clip(min=1.0)
    training = samples(dataset / "train.jsonl", names, mean, scale)
    validation = samples(dataset / "validation.jsonl", names, mean, scale)
    loader = DataLoader(TensorDataset(*training), batch_size=64, shuffle=True)
    model = CandidateTransformer(len(names))
    optimizer = torch.optim.AdamW(model.parameters(), lr=3e-4, weight_decay=1e-3)
    best_loss = float("inf")
    best_state = None
    best_epoch = 0
    logs = []
    for epoch in range(epochs):
        model.train()
        for past, current, history_valid, valid, labels in loader:
            optimizer.zero_grad()
            loss = pairwise_loss(model(past, current, history_valid, valid), labels, valid)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optimizer.step()
        val_loss, accuracy = validate(model, validation)
        logs.append({"epoch": epoch, "validation_loss": val_loss, "top_choice_accuracy": accuracy})
        print(json.dumps(logs[-1]), flush=True)
        if val_loss < best_loss - 1e-5:
            best_loss, best_epoch = val_loss, epoch
            best_state = deepcopy(model.state_dict())
        if epoch - best_epoch >= 6:
            break
    output.mkdir(parents=True, exist_ok=False)
    torch.save(best_state, output / "model.pt")
    metadata = {
        "schema_version": 1, "model_type": "transformer", "features_version": manifest["features_version"],
        "simulation_version": manifest["simulation_version"], "dataset_manifest": manifest,
        "feature_names": names, "mean": mean.tolist(), "scale": scale.tolist(),
        "architecture": {"history": 8, "width": 64, "layers": 2},
        "parameters": {"seed": seed, "epochs": epochs, "batch_size": 64, "learning_rate": 3e-4, "weight_decay": 1e-3},
        "parameter_count": sum(parameter.numel() for parameter in model.parameters()),
        "torch_version": torch.__version__, "best_epoch": best_epoch,
        "train_sha256": hashlib.sha256((dataset / "train.jsonl").read_bytes()).hexdigest(),
        "validation_sha256": hashlib.sha256((dataset / "validation.jsonl").read_bytes()).hexdigest(),
        "logs": logs,
    }
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dataset", type=Path, default=Path("outputs/dataset-v6-memory"))
    parser.add_argument("--output", type=Path, default=Path("outputs/transformer-v1"))
    parser.add_argument("--epochs", type=int, default=30)
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()
    train(args.dataset, args.output, args.epochs, args.seed)

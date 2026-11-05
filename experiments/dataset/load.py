"""Validated grouped arrays for a fixed dataset version."""
import json
from pathlib import Path

import numpy as np


def load_split(path: Path, feature_names: list[str] | None = None):
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    if not rows:
        raise ValueError(f"empty dataset: {path}")
    names = feature_names or sorted(rows[0]["features"])
    groups = {}
    for row in rows:
        if row["schema_version"] != 1 or row["features_version"] not in (1, 2, 3, 4, 5):
            raise ValueError("unsupported dataset version")
        if sorted(row["features"]) != names or not row["valid"]:
            raise ValueError("inconsistent features or invalid candidate")
        groups.setdefault((row["episode_id"], row["decision_id"]), []).append(row)
    ordered = []
    sizes = []
    for group in groups.values():
        if len({row["candidate_id"] for row in group}) != len(group):
            raise ValueError("duplicate candidate ID")
        sizes.append(len(group))
        ordered.extend(group)
    x = np.asarray([[row["features"][name] for name in names] for row in ordered], dtype=np.float32)
    y = np.asarray([row["relevance"] for row in ordered], dtype=np.float32)
    if not np.isfinite(x).all() or not np.isfinite(y).all() or (y < 0).any():
        raise ValueError("invalid numeric feature or relevance")
    return x, y, sizes, names

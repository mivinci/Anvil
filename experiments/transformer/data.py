import json
from pathlib import Path

import numpy as np
import torch

from dataset.load import load_split
from scenarios.world import ACTIONS


def samples(path: Path, names: list[str], mean, scale, history: int = 8):
    # Reuse numeric/schema/group validation used by XGBoost.
    load_split(path, names)
    episodes = {}
    for line in path.read_text().splitlines():
        row = json.loads(line)
        episodes.setdefault(row["episode_id"], {}).setdefault(row["decision_id"], []).append(row)
    result = []
    for decisions in episodes.values():
        past = []
        for tick, rows in sorted(decisions.items()):
            if sum(row["executed"] for row in rows) != 1:
                raise ValueError("history requires exactly one executed candidate")
            vectors = np.asarray([[row["features"][name] for name in names] for row in rows], dtype=np.float32)
            vectors = np.clip((vectors - mean) / scale, -10, 10)
            current = np.zeros((len(ACTIONS), len(names)), dtype=np.float32)
            current[:len(rows)] = vectors
            valid = np.arange(len(ACTIONS)) < len(rows)
            labels = np.zeros(len(ACTIONS), dtype=np.float32)
            labels[:len(rows)] = [row["relevance"] for row in rows]
            previous = np.zeros((history, len(names)), dtype=np.float32)
            history_mask = np.zeros(history, dtype=bool)
            retained = past[-history:]
            if retained:
                previous[-len(retained):] = retained
                history_mask[-len(retained):] = True
            result.append((previous, current, history_mask, valid, labels))
            chosen = next(index for index, row in enumerate(rows) if row["executed"])
            past.append(vectors[chosen])
    return tuple(torch.from_numpy(np.stack([sample[index] for sample in result])) for index in range(5))

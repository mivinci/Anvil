from collections import deque
import json
from pathlib import Path

import numpy as np
import torch

from dataset.generate import features
from .model import CandidateTransformer


class TransformerPolicy:
    def __init__(self, directory: Path):
        torch.set_num_threads(1)
        self.metadata = json.loads((directory / "metadata.json").read_text())
        if self.metadata["schema_version"] != 1 or self.metadata["features_version"] not in (1, 2, 3, 4, 5):
            raise ValueError("unsupported model metadata")
        self.names = self.metadata["feature_names"]
        self.mean = np.asarray(self.metadata["mean"], dtype=np.float32)
        self.scale = np.asarray(self.metadata["scale"], dtype=np.float32)
        self.model = CandidateTransformer(len(self.names), **self.metadata["architecture"])
        self.model.load_state_dict(torch.load(directory / "model.pt", map_location="cpu", weights_only=True))
        self.model.eval()
        self.past = deque(maxlen=self.model.history)

    def reset(self):
        self.past.clear()

    def choose(self, world):
        candidates = world.candidates()
        rows = [features(world, action, self.metadata["features_version"]) for action in candidates]
        vectors = np.asarray([[row[name] for name in self.names] for row in rows], dtype=np.float32)
        vectors = np.clip((vectors - self.mean) / self.scale, -10, 10)
        history = np.zeros((self.model.history, len(self.names)), dtype=np.float32)
        history_mask = np.zeros(self.model.history, dtype=bool)
        if self.past:
            history[-len(self.past):] = list(self.past)
            history_mask[-len(self.past):] = True
        with torch.inference_mode():
            scores = self.model(
                torch.from_numpy(history)[None], torch.from_numpy(vectors)[None],
                torch.from_numpy(history_mask)[None], torch.ones((1, len(candidates)), dtype=torch.bool),
            )[0].numpy()
        if not np.isfinite(scores).all():
            raise ValueError("non-finite model scores")
        chosen = int(np.argmax(scores))
        self.past.append(vectors[chosen].copy())
        return candidates[chosen]

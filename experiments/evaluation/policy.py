import json
from pathlib import Path

import numpy as np
import xgboost as xgb

from dataset.generate import features
from scenarios.world import World


class RankingPolicy:
    def __init__(self, directory: Path):
        self.metadata = json.loads((directory / "metadata.json").read_text())
        if self.metadata["schema_version"] != 1 or self.metadata["features_version"] not in (1, 2, 3, 4, 5):
            raise ValueError("unsupported model metadata")
        self.model = xgb.XGBRanker()
        self.model.load_model(directory / "model.ubj")
        self.names = self.metadata["feature_names"]

    def choose(self, world: World) -> str:
        candidates = world.candidates()
        rows = [features(world, action, self.metadata["features_version"]) for action in candidates]
        matrix = np.asarray([[row[name] for name in self.names] for row in rows], dtype=np.float32)
        scores = self.model.predict(matrix)
        if not np.isfinite(scores).all():
            raise ValueError("model returned non-finite scores")
        return candidates[int(np.argmax(scores))]


def load_policy(directory: Path):
    metadata = json.loads((directory / "metadata.json").read_text())
    model_type = metadata.get("model_type", "xgboost")
    if model_type == "transformer":
        from transformer.policy import TransformerPolicy
        return TransformerPolicy(directory)
    if model_type == "xgboost":
        return RankingPolicy(directory)
    raise ValueError("unsupported model type")


def policy_name(policy):
    return policy.metadata.get("model_type", "xgboost")


def model_digest(directory: Path):
    import hashlib

    metadata = json.loads((directory / "metadata.json").read_text())
    filename = "model.pt" if metadata.get("model_type") == "transformer" else "model.ubj"
    return hashlib.sha256((directory / filename).read_bytes()).hexdigest()

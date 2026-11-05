import json
from pathlib import Path
import tempfile
import unittest

import numpy as np
import torch

from transformer.data import samples
from transformer.model import CandidateTransformer, pairwise_loss


class TransformerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)

    def test_candidate_permutation_and_padding(self):
        torch.manual_seed(0)
        model = CandidateTransformer(3, history=2, width=16, layers=1).eval()
        history = torch.zeros(1, 2, 3)
        current = torch.randn(1, 3, 3)
        history_valid = torch.tensor([[False, True]])
        valid = torch.tensor([[True, True, False]])
        with torch.no_grad():
            scores = model(history, current, history_valid, valid)
            permuted = model(history, current[:, [1, 0, 2]], history_valid, valid)
            modified = current.clone()
            modified[:, 2] = 1e6
            padded_changed = model(history, modified, history_valid, valid)
        self.assertTrue(torch.allclose(scores[:, [1, 0]], permuted[:, :2], atol=1e-6))
        self.assertTrue(torch.allclose(scores[:, :2], padded_changed[:, :2], atol=1e-6))
        self.assertLess(float(scores[0, 2]), -1e8)
        loss = pairwise_loss(scores, torch.tensor([[1., 0., 999.]]), valid)
        self.assertTrue(torch.isfinite(loss))

    def test_saved_policy_can_reset_without_carrying_history_to_new_episode(self):
        from scenarios.tasks import Task
        from scenarios.world import World
        from transformer.policy import TransformerPolicy

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            architecture = {"history": 2, "width": 16, "layers": 1}
            model = CandidateTransformer(3, **architecture)
            torch.save(model.state_dict(), root / "model.pt")
            (root / "metadata.json").write_text(json.dumps({
                "schema_version": 1, "features_version": 5,
                "feature_names": ["move_x", "move_y", "task"],
                "mean": [0, 0, 0], "scale": [1, 1, 1],
                "architecture": architecture,
            }))
            policy = TransformerPolicy(root)
            world = World(0, Task.FOLLOW)
            first = policy.choose(world)
            self.assertEqual(len(policy.past), 1)
            policy.reset()
            self.assertEqual(len(policy.past), 0)
            self.assertEqual(policy.choose(world), first)

    def test_history_uses_only_prior_executed_rows_and_resets_per_episode(self):
        rows = []
        for episode in ["a", "b"]:
            for tick in range(2):
                for candidate in range(2):
                    rows.append({
                        "schema_version": 1, "features_version": 5,
                        "episode_id": episode, "decision_id": tick,
                        "candidate_id": candidate, "valid": True,
                        "features": {"x": tick * 10 + candidate},
                        "relevance": candidate, "executed": candidate == 1,
                    })
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "data.jsonl"
            path.write_text("".join(json.dumps(row) + "\n" for row in rows))
            past, _, valid, _, _ = samples(path, ["x"], np.array([0.]), np.array([1.]), history=2)
        self.assertFalse(valid[0].any())
        self.assertEqual(past[1, -1, 0], 1)
        self.assertEqual(valid[1].sum(), 1)
        self.assertFalse(valid[2].any())


if __name__ == "__main__":
    unittest.main()

import json
import tempfile
import unittest
from pathlib import Path

from dataset.load import load_split
from dataset.generate import candidate_scores, features, generate
from scenarios.tasks import Task
from scenarios.world import World, baseline


class SimulationTests(unittest.TestCase):
    def test_seeded_episode_and_clone_are_reproducible(self):
        first = World(13, Task.COLLECT)
        second = World(13, Task.COLLECT)
        before = first.clone()
        self.assertEqual(candidate_scores(first, 8), candidate_scores(second, 8))
        self.assertEqual(first.__dict__, before.__dict__)
        while not first.done:
            action = baseline(first)
            self.assertEqual(first.step(action), second.step(action))
            self.assertEqual(first.__dict__, second.__dict__)

    def test_features_do_not_expose_hidden_state(self):
        first = World(21, Task.FOLLOW)
        second = first.clone()
        observation = first.observe()
        hidden = next(
            (x, y) for x in range(1, 11) for y in range(1, 11)
            if (x, y) not in observation.cells
        )
        second.resources[hidden] = 999
        for version in [1, 2, 3, 4, 5]:
            self.assertEqual(features(first, "wait", version), features(second, "wait", version))

    def test_collision_resource_and_explosion(self):
        world = World(0, Task.COLLECT)
        world.position = (1, 1)
        self.assertNotIn("west", world.candidates())
        world.step("west")
        self.assertEqual(world.position, (1, 1))
        world.resources = {(1, 2): 1}
        world.creeper = (10, 10)
        world.step("collect")
        self.assertEqual(world.collected, 1)
        self.assertNotIn("collect", world.candidates())
        world.creeper = world.position
        world.fuse = 1
        world.step("wait")
        self.assertEqual(world.reason, "death")

    def test_loader_rejects_inconsistent_features(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "data.jsonl"
            row = {
                "schema_version": 1, "features_version": 1,
                "episode_id": "train-0", "decision_id": 0,
                "candidate_id": 0, "valid": True,
                "features": {"x": 1}, "relevance": 0,
            }
            other = {**row, "candidate_id": 1, "features": {"y": 2}}
            path.write_text(json.dumps(row) + "\n" + json.dumps(other) + "\n")
            with self.assertRaisesRegex(ValueError, "inconsistent features"):
                load_split(path)

    def test_augmentation_only_adds_training_trajectories(self):
        class WaitingPolicy:
            metadata = {"test_policy": "wait"}

            def choose(self, world):
                return "wait"

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            base = generate(root / "base", 3, 2, feature_version=2)
            augmented = generate(root / "augmented", 3, 2, feature_version=2, actor=WaitingPolicy(), threat_starts=2, follow_starts=4)
            self.assertGreater(augmented["splits"]["train"]["rows"], base["splits"]["train"]["rows"])
            for split in ["validation", "test"]:
                self.assertEqual(
                    (root / "base" / f"{split}.jsonl").read_bytes(),
                    (root / "augmented" / f"{split}.jsonl").read_bytes(),
                )

    def test_escape_margin_respects_candidate_step_and_obstacle(self):
        from dataset.action_features import threat_features

        world = World(0, Task.ESCAPE)
        world.position = (4, 4)
        world.creeper = (4, 5)
        world.fuse = 2
        world.walls = {(x, y) for x in range(12) for y in range(12)
                       if x in (0, 11) or y in (0, 11)}
        # Going north needs one more move to reach distance three.
        north = threat_features(world, "north")
        self.assertEqual(north["escape_steps_after"], 1)
        self.assertEqual(north["escape_time_margin"], 0)
        world.fuse = 1
        self.assertEqual(threat_features(world, "north")["escape_time_margin"], -1)
        world.walls.add((4, 2))
        self.assertGreaterEqual(threat_features(world, "north")["escape_steps_after"], 1)

    def test_discount_distinguishes_delaying_the_same_pickup(self):
        world = World(0, Task.COLLECT)
        world.position = (1, 1)
        world.recent_positions = [world.position]
        world.visited = {world.position}
        world.fuse = 0
        world.resources = {(4, 1): 1}
        world.walls = {(x, y) for x in range(12) for y in range(12)
                       if y != 1 or x in (0, 11)}
        undiscounted = dict(candidate_scores(world, 8, label_version=1))
        discounted = dict(candidate_scores(world, 8, label_version=2))
        self.assertAlmostEqual(undiscounted["east"], undiscounted["wait"])
        self.assertGreater(discounted["east"], discounted["wait"])

    def test_task_mask_forbids_unrelated_collection(self):
        for task in Task:
            world = World(0, task, task_action_mask=True)
            world.resources = {world.position: 1}
            self.assertEqual("collect" in world.candidates(), task == Task.COLLECT)

    def test_follow_memory_does_not_read_unseen_player_or_route(self):
        world = World(0, Task.FOLLOW)
        world.position = (1, 1)
        world.player = (10, 10)
        world.player_last_seen = ((2, 1), 0)
        world.player_previous_seen = ((1, 1), -1)
        other = world.clone()
        other.player = (9, 9)
        other.player_route = [(9, 10)]
        self.assertEqual(features(world, "wait", 5), features(other, "wait", 5))
        self.assertEqual(features(world, "wait", 5)["last_seen_x"], 1)

    def test_follow_profiles_reset_memory_and_keep_initialization_reproducible(self):
        from scenarios.follow import PROFILES, configure_follow

        for profile in PROFILES:
            first = World(12, Task.FOLLOW, task_action_mask=True)
            second = first.clone()
            configure_follow(first, profile)
            configure_follow(second, profile)
            self.assertEqual(first.__dict__, second.__dict__)
            self.assertEqual(first.fuse, 0)
            self.assertIsNone(first.player_previous_seen)
            if profile == "distant_stationary":
                self.assertIsNone(first.observe().player)
            else:
                self.assertEqual(first.observe().player, first.player)
                self.assertEqual(first.player_last_seen, (first.player, 0))

    def test_horizon_change_preserves_trajectories_and_features(self):
        from dataset.compare import compare_windows

        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "short", Path(directory) / "long"
            generate(first, 3, 2, feature_version=5, label_version=2)
            generate(second, 3, 8, feature_version=5, label_version=2)
            counts = compare_windows(first, second)
            self.assertGreater(counts["train"]["changed_labels"], 0)
            path = second / "train.jsonl"
            lines = path.read_text().splitlines()
            row = json.loads(lines[0])
            row["features"]["move_x"] += 1
            lines[0] = json.dumps(row)
            path.write_text("\n".join(lines) + "\n")
            with self.assertRaisesRegex(ValueError, "features changed"):
                compare_windows(first, second)

    def test_generation_has_disjoint_maps_and_complete_groups(self):
        with tempfile.TemporaryDirectory() as directory:
            manifest = generate(Path(directory) / "data", 3, 2)
            maps = [identity for split in manifest["splits"].values() for identity in split["maps"]]
            self.assertEqual(len(maps), len(set(maps)))
            for split in manifest["splits"]:
                rows = [json.loads(line) for line in (Path(directory) / "data" / f"{split}.jsonl").read_text().splitlines()]
                groups = {}
                for row in rows:
                    groups.setdefault((row["episode_id"], row["decision_id"]), []).append(row)
                for group in groups.values():
                    self.assertEqual(sum(row["executed"] for row in group), 1)
                    self.assertTrue(all(row["valid"] for row in group))
                    self.assertTrue(all((row["outcome"] is not None) == row["executed"] for row in group))


if __name__ == "__main__":
    unittest.main()

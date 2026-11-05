import unittest

from evaluation.suite import build_cases
from scenarios.tasks import Task


class SuiteTests(unittest.TestCase):
    def test_cases_are_balanced_and_reproducible(self):
        manifest = {"splits": {"train": {"maps": []}}}
        first = build_cases(manifest, 12, 3_000_000)
        self.assertEqual(first, build_cases(manifest, 12, 3_000_000))
        self.assertEqual(len({case["map_id"] for case in first}), 12)
        self.assertEqual(
            {(case["task"], case["initial_fuse"]) for case in first},
            {(task.value, fuse) for task in Task for fuse in [2, 4, 8, 12]},
        )

    def test_existing_maps_are_rejected(self):
        manifest = {"splits": {"train": {"maps": []}}}
        case = build_cases(manifest, 12, 3_000_000)[0]
        manifest["splits"]["train"]["maps"].append(case["map_id"])
        with self.assertRaisesRegex(ValueError, "overlaps"):
            build_cases(manifest, 12, 3_000_000)


if __name__ == "__main__":
    unittest.main()

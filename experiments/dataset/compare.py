"""Verify that a label-window experiment changes only candidate labels."""
import argparse
from itertools import zip_longest
import json
from pathlib import Path

LABEL_FIELDS = {"relevance", "rollout_return"}


def compare_windows(first: Path, second: Path):
    old = json.loads((first / "manifest.json").read_text())
    new = json.loads((second / "manifest.json").read_text())
    if {k: v for k, v in old.items() if k != "horizon"} != {
        k: v for k, v in new.items() if k != "horizon"
    }:
        raise ValueError("dataset settings differ beyond horizon")
    counts = {}
    for split in old["splits"]:
        count = changed = 0
        with (first / f"{split}.jsonl").open() as a, (second / f"{split}.jsonl").open() as b:
            for left, right in zip_longest(a, b):
                if left is None or right is None:
                    raise ValueError("dataset row count changed")
                x, y = json.loads(left), json.loads(right)
                if {k: v for k, v in x.items() if k not in LABEL_FIELDS} != {
                    k: v for k, v in y.items() if k not in LABEL_FIELDS
                }:
                    raise ValueError(f"trajectory or features changed in {split} row {count}")
                count += 1
                changed += any(x[k] != y[k] for k in LABEL_FIELDS)
        counts[split] = {"rows": count, "changed_labels": changed}
    return counts


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("first", type=Path)
    parser.add_argument("second", type=Path)
    args = parser.parse_args()
    print(json.dumps(compare_windows(args.first, args.second), indent=2))

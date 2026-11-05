"""Seeded maps. Split-specific seeds are supplied by the dataset generator."""
import random

WIDTH = 12
HEIGHT = 12


def make_map(seed: int) -> tuple[set[tuple[int, int]], list[tuple[int, int]]]:
    rng = random.Random(seed)
    walls = {
        (x, y)
        for x in range(WIDTH)
        for y in range(HEIGHT)
        if x in (0, WIDTH - 1) or y in (0, HEIGHT - 1)
    }
    walls.update(
        (x, y)
        for x in range(2, WIDTH - 2)
        for y in range(2, HEIGHT - 2)
        if rng.random() < 0.18
    )
    # Choose all initial objects from one connected component.
    reachable = {(1, 1)}
    frontier = [(1, 1)]
    while frontier:
        x, y = frontier.pop()
        for point in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]:
            if point not in walls and point not in reachable:
                reachable.add(point)
                frontier.append(point)
    floor = sorted(reachable)
    rng.shuffle(floor)
    return walls, floor

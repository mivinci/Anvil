"""Candidate consequences computed only from the current local observation."""
from collections import deque

from scenarios.world import MOVES, distance


def path_distance(start, goals, floor):
    queue = deque([(start, 0)])
    seen = {start}
    while queue:
        point, steps = queue.popleft()
        if point in goals:
            return steps
        for dx, dy in MOVES.values():
            neighbor = (point[0] + dx, point[1] + dy)
            if neighbor in floor and neighbor not in seen:
                seen.add(neighbor)
                queue.append((neighbor, steps + 1))
    return -1


def candidate_features(world, action):
    observation = world.observe()
    dx, dy = MOVES.get(action, (0, 0))
    point = (observation.position[0] + dx, observation.position[1] + dy)
    floor = {p for p, kind in observation.cells.items() if kind != "wall"}
    resources = {p for p, kind in observation.cells.items() if kind == "resource"}
    collect_goals = {p for p in floor if any(distance(p, resource) <= 1 for resource in resources)}
    follow_goals = {p for p in floor if observation.player is not None and distance(p, observation.player) <= 2}
    return {
        "next_player_distance": distance(point, observation.player) if observation.player else -1,
        "next_creeper_distance": distance(point, observation.creeper) if observation.creeper else -1,
        "resource_steps_before": path_distance(observation.position, collect_goals, floor),
        "resource_steps_after": path_distance(point, collect_goals, floor),
        "follow_steps_before": path_distance(observation.position, follow_goals, floor),
        "follow_steps_after": path_distance(point, follow_goals, floor),
        "frontier_steps_after": path_distance(point, floor - world.visited, floor),
        "next_visited": int(point in world.visited),
        "next_degree": sum((point[0] + mx, point[1] + my) in floor for mx, my in MOVES.values()),
    }


def threat_features(world, action):
    observation = world.observe()
    if observation.creeper is None:
        return {"escape_steps_after": -1, "escape_time_margin": -99, "inside_blast_after": 0}
    dx, dy = MOVES.get(action, (0, 0))
    point = (observation.position[0] + dx, observation.position[1] + dy)
    floor = {p for p, kind in observation.cells.items() if kind != "wall"}
    goals = {p for p in floor if distance(p, observation.creeper) > 2}
    steps = path_distance(point, goals, floor)
    return {
        "escape_steps_after": steps,
        "escape_time_margin": observation.fuse - 1 - steps if steps >= 0 else -99,
        "inside_blast_after": int(distance(point, observation.creeper) <= 2),
    }


def follow_memory_features(world, action):
    observation = world.observe()
    dx, dy = MOVES.get(action, (0, 0))
    point = (observation.position[0] + dx, observation.position[1] + dy)
    floor = {p for p, kind in observation.cells.items() if kind != "wall"}
    last = world.player_last_seen
    previous = world.player_previous_seen
    velocity = (0, 0)
    if last and previous and last[1] == previous[1] + 1:
        velocity = (last[0][0] - previous[0][0], last[0][1] - previous[0][1])
    seen_position = last[0] if last else None
    # One-step extrapolation is a feature, not access to the player's route.
    predicted = ((seen_position[0] + velocity[0], seen_position[1] + velocity[1])
                 if observation.player is not None else None)
    predicted_goals = {p for p in floor if predicted and distance(p, predicted) <= 2}
    last_goals = {p for p in floor if seen_position and distance(p, seen_position) <= 2}
    return {
        "player_velocity_x": velocity[0],
        "player_velocity_y": velocity[1],
        "last_seen_known": int(last is not None),
        "last_seen_age": world.tick - last[1] if last else -1,
        "last_seen_x": seen_position[0] - observation.position[0] if last else 0,
        "last_seen_y": seen_position[1] - observation.position[1] if last else 0,
        "predicted_follow_steps_after": path_distance(point, predicted_goals, floor),
        "last_seen_follow_steps_after": path_distance(point, last_goals, floor),
        "follow_progress": world.follow_ticks,
    }

"""Follow curricula. Full-map queries are used only to initialize scenarios."""
import random

from .maps import HEIGHT, WIDTH
from .tasks import Task
from .world import MOVES, RADIUS, World, distance

PROFILES = ("leave_view", "distant_stationary", "moving_turns", "visible_stationary")


def configure_follow(world: World, profile: str, variant: int = 0):
    if world.task != Task.FOLLOW or profile not in PROFILES or world.tick != 0:
        raise ValueError("follow scenario requires a fresh follow world and a known profile")
    reachable = {world.position}
    frontier = [world.position]
    while frontier:
        point = frontier.pop()
        for dx, dy in MOVES.values():
            neighbor = (point[0] + dx, point[1] + dy)
            if (0 <= neighbor[0] < WIDTH and 0 <= neighbor[1] < HEIGHT
                    and neighbor not in world.walls and neighbor not in reachable):
                reachable.add(neighbor)
                frontier.append(neighbor)
    floor = sorted(reachable)
    rng = random.Random(world.seed * 1009 + variant)

    def visibility_distance(point):
        return max(abs(point[0] - world.position[0]), abs(point[1] - world.position[1]))

    if profile == "distant_stationary":
        starts = sorted(floor, key=lambda point: (-distance(world.position, point), point))[:5]
        world.player = rng.choice(starts)
        world.player_route = [world.player]
    else:
        target_radius = 2 if profile == "moving_turns" else RADIUS
        starts = [point for point in floor if visibility_distance(point) == target_radius]
        if not starts:
            raise ValueError("map has no suitable follow start")
        world.player = rng.choice(starts)
        if profile == "leave_view":
            destinations = sorted(floor, key=lambda point: (-distance(world.position, point), point))
        else:
            destinations = sorted(floor, key=lambda point: (-distance(world.player, point), point))
        world.player_route = [destinations[0], destinations[-1]]
        if profile == "visible_stationary":
            # Keep the initial target still; existing walls determine the detour.
            world.player_route = [world.player]
    world.fuse = 0
    world.player_last_seen = None
    world.player_previous_seen = None
    world._remember_player()

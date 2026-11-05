use crate::{Observation, Position};

const DANGER_RADIUS: u32 = 6;
const RETREAT_BLOCKS: i32 = 8;

pub(crate) fn destination(observation: &Observation) -> Option<Position> {
  let Position(x, y, z) = observation.position;
  let (dx, dz) = observation
    .ignited_creepers
    .iter()
    .filter(|&&position| observation.position.within(position, DANGER_RADIUS))
    .map(|&Position(cx, _, cz)| (x - cx, z - cz))
    .min_by_key(|&(dx, dz)| dx * dx + dz * dz)?;
  // Choose a horizontal waypoint. The adapter is responsible for path validity.
  if dx.abs() >= dz.abs() {
    let direction = if dx < 0 { -1 } else { 1 };
    Some(Position(x.saturating_add(direction * RETREAT_BLOCKS), y, z))
  } else {
    Some(Position(
      x,
      y,
      z.saturating_add(dz.signum() * RETREAT_BLOCKS),
    ))
  }
}

use crate::{EntityId, Observation, Position};
use std::time::Duration;

const TARGET_GRACE: Duration = Duration::from_secs(3);

pub(crate) struct Follow {
  pub target: EntityId,
  pub distance: u32,
  missing_since: Duration,
}

impl Follow {
  pub fn new(target: EntityId, distance: u32, now: Duration) -> Self {
    Self {
      target,
      distance,
      missing_since: now,
    }
  }

  /// Returns the desired destination. None means hold position.
  pub fn destination(&mut self, observation: &Observation, now: Duration) -> Option<Position> {
    let target = observation
      .nearby_players
      .iter()
      .find_map(|(id, position)| (*id == self.target).then_some(*position))?;
    self.missing_since = now;
    (!observation.position.within(target, self.distance)).then_some(target)
  }

  pub fn reset_grace(&mut self, now: Duration) {
    self.missing_since = now;
  }

  pub fn expired(&self, now: Duration) -> bool {
    now.saturating_sub(self.missing_since) >= TARGET_GRACE
  }
}

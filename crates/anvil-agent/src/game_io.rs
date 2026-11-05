pub type ActionId = u64;
pub type EntityId = u128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
  OakLog,
}

/// Position in the current dimension, in blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position(pub i32, pub i32, pub i32);

impl Position {
  pub(crate) fn within(self, other: Self, radius: u32) -> bool {
    let dx = i128::from(self.0) - i128::from(other.0);
    let dy = i128::from(self.1) - i128::from(other.1);
    let dz = i128::from(self.2) - i128::from(other.2);
    dx * dx + dy * dy + dz * dz <= i128::from(radius).pow(2)
  }
}

#[derive(Debug)]
pub struct Observation {
  pub tick: u64,
  pub position: Position,
  /// Positions of observed creepers that have started their fuse.
  pub ignited_creepers: Vec<Position>,
  pub nearby_players: Vec<(EntityId, Position)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
  /// Approach the target without breaking or placing blocks.
  Navigate {
    id: ActionId,
    target: Position,
    distance: u32,
  },
  /// Collect one unit. Finished confirms that the unit was acquired.
  CollectOne {
    id: ActionId,
    resource: Resource,
  },
  Cancel {
    id: ActionId,
  },
}

/// An adapter reports these only once the action can no longer drive movement.
/// After receiving Cancel, it must acknowledge with Stopped, even if the action
/// already finished. A racing completion is not a cancellation acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionResult {
  Finished,
  Failed,
  ResourceUnavailable,
  InventoryFull,
  Stopped,
}

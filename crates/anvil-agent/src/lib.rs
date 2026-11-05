//! Deterministic execution of player skills over a game adapter.
pub mod contract;
mod executor;
mod game_io;
mod skills;

pub use executor::{Command, Event, Executor, Output, Status};
pub use game_io::{Action, ActionId, ActionResult, EntityId, Observation, Position, Resource};

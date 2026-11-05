use crate::{
  Action, ActionId, ActionResult, EntityId, Observation, Position, Resource,
  skills::{avoid, follow::Follow},
};
use std::time::Duration;

const STOP_TIMEOUT: Duration = Duration::from_secs(2);
const OBSERVATION_TIMEOUT: Duration = Duration::from_secs(1);
const MOVEMENT_TIMEOUT: Duration = Duration::from_secs(3);
const COLLECTION_TIMEOUT: Duration = Duration::from_secs(10);
const SAFE_WINDOW: Duration = Duration::from_millis(500);
const REPATH_DISTANCE: u32 = 2;

pub enum Command {
  Follow { target: EntityId, distance: u32 },
  Collect { resource: Resource, quantity: u32 },
  Stop,
}

pub enum Event {
  Command(Command),
  Observation(Observation),
  ActionResult {
    id: ActionId,
    result: ActionResult,
  },
  /// The adapter has torn down the session and all its actions.
  Disconnected,
  Timer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
  Busy,
  InvalidQuantity,
  CollectionProgress { collected: u32, quantity: u32 },
  CollectionCompleted { quantity: u32 },
  CollectionFailed,
  ResourceUnavailable,
  InventoryFull,
  Stopped,
  StopUnconfirmed,
  TargetLost,
  ObservationExpired,
  NavigationStalled,
  NavigationFailed,
  AvoidanceStarted,
  AvoidanceEnded,
  AvoidanceFailed,
  Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
  Action(Action),
  Status(Status),
}

enum Task {
  Follow(Follow),
  Collect {
    resource: Resource,
    quantity: u32,
    collected: u32,
  },
}

struct ActiveAction {
  id: ActionId,
  target: Option<Position>,
  cancelling: Option<Cancellation>,
  last_position: Position,
  last_progress: Duration,
}

struct Cancellation {
  deadline: Duration,
  outcome: Option<Status>,
  reported_timeout: bool,
}

#[derive(Default)]
pub struct Executor {
  task: Option<Task>,
  avoiding: bool,
  safe_since: Option<Duration>,
  last_observed_at: Duration,
  active_action: Option<ActiveAction>,
  last_tick: Option<u64>,
  next_action: ActionId,
}

impl Executor {
  /// Process one event using monotonic time since an arbitrary origin. The
  /// caller serializes events and supplies timer events even without observations.
  pub fn step(&mut self, event: Event, now: Duration) -> Vec<Output> {
    let mut output = Vec::new();
    if self.task.is_some()
      && !matches!(&event, Event::Command(Command::Stop) | Event::Disconnected)
      && now.saturating_sub(self.last_observed_at) >= OBSERVATION_TIMEOUT
    {
      self.end(Status::ObservationExpired, now, &mut output);
    }
    match event {
      Event::Command(Command::Follow { target, distance }) => {
        if self.task.is_some() || self.active_action.is_some() {
          output.push(Output::Status(Status::Busy));
        } else {
          self.last_observed_at = now;
          self.task = Some(Task::Follow(Follow::new(target, distance, now)));
        }
      }
      Event::Command(Command::Collect { resource, quantity }) => {
        if self.task.is_some() || self.active_action.is_some() {
          output.push(Output::Status(Status::Busy));
        } else if quantity == 0 {
          output.push(Output::Status(Status::InvalidQuantity));
        } else {
          self.last_observed_at = now;
          self.task = Some(Task::Collect {
            resource,
            quantity,
            collected: 0,
          });
        }
      }
      Event::Command(Command::Stop) => self.end(Status::Stopped, now, &mut output),
      Event::Observation(observation) => {
        if self.last_tick.is_none_or(|tick| observation.tick > tick) {
          self.last_tick = Some(observation.tick);
          self.last_observed_at = now;
          self.observe(&observation, now, &mut output);
        }
      }
      Event::ActionResult { id, result } => self.action_result(id, result, &mut output),
      Event::Disconnected => {
        self.task = None;
        self.avoiding = false;
        self.safe_since = None;
        self.active_action = None;
        self.last_tick = None;
        output.push(Output::Status(Status::Disconnected));
      }
      Event::Timer => {}
    }
    if !self.avoiding && matches!(&self.task, Some(Task::Follow(follow)) if follow.expired(now)) {
      self.end(Status::TargetLost, now, &mut output);
    }
    if let Some(action) = &self.active_action {
      let timeout = if action.target.is_some() {
        MOVEMENT_TIMEOUT
      } else {
        COLLECTION_TIMEOUT
      };
      if action.cancelling.is_none() && now.saturating_sub(action.last_progress) >= timeout {
        let outcome = if self.avoiding {
          Status::AvoidanceFailed
        } else if action.target.is_none() {
          Status::CollectionFailed
        } else {
          Status::NavigationStalled
        };
        self.end(outcome, now, &mut output);
      }
    }
    if let Some(cancel) = self
      .active_action
      .as_mut()
      .and_then(|movement| movement.cancelling.as_mut())
      && now >= cancel.deadline
      && !cancel.reported_timeout
    {
      cancel.reported_timeout = true;
      self.task = None;
      self.avoiding = false;
      output.push(Output::Status(Status::StopUnconfirmed));
    }
    output
  }

  fn observe(&mut self, observation: &Observation, now: Duration, output: &mut Vec<Output>) {
    let Some(task) = &mut self.task else {
      return;
    };
    let retreat = avoid::destination(observation);
    if let Some(movement) = &mut self.active_action
      && movement.target.is_some()
      && observation.position != movement.last_position
    {
      movement.last_position = observation.position;
      movement.last_progress = now;
    }
    if retreat.is_some() {
      self.safe_since = None;
    } else if self.avoiding && now.saturating_sub(*self.safe_since.get_or_insert(now)) < SAFE_WINDOW
    {
      return;
    }
    let avoiding = retreat.is_some();
    if avoiding != self.avoiding {
      self.avoiding = avoiding;
      if let Task::Follow(follow) = task {
        follow.reset_grace(now);
      }
      output.push(Output::Status(if avoiding {
        Status::AvoidanceStarted
      } else {
        Status::AvoidanceEnded
      }));
      if self.active_action.is_some() {
        self.cancel(None, now, output);
        return;
      }
    }
    let (destination, distance) = if avoiding {
      (retreat, 0)
    } else {
      match task {
        Task::Follow(follow) => (follow.destination(observation, now), follow.distance),
        Task::Collect { resource, .. } => {
          if self.active_action.is_none() {
            let resource = *resource;
            let id = self.start_action(None, observation.position, now);
            output.push(Output::Action(Action::CollectOne { id, resource }));
          }
          return;
        }
      }
    };
    if let Some(movement) = &self.active_action {
      if !avoiding
        && movement.target.is_some_and(|current| {
          destination.is_none_or(|target| !target.within(current, REPATH_DISTANCE))
        })
      {
        self.cancel(None, now, output);
      }
    } else if let Some(target) = destination {
      let id = self.start_action(Some(target), observation.position, now);
      output.push(Output::Action(Action::Navigate {
        id,
        target,
        distance,
      }));
    }
  }

  fn start_action(
    &mut self,
    target: Option<Position>,
    position: Position,
    now: Duration,
  ) -> ActionId {
    self.next_action += 1;
    let id = self.next_action;
    self.active_action = Some(ActiveAction {
      id,
      target,
      cancelling: None,
      last_position: position,
      last_progress: now,
    });
    id
  }

  fn end(&mut self, outcome: Status, now: Duration, output: &mut Vec<Output>) {
    self.task = None;
    self.avoiding = false;
    self.safe_since = None;
    if self.active_action.is_some() {
      self.cancel(Some(outcome), now, output);
    } else {
      output.push(Output::Status(outcome));
    }
  }

  fn cancel(&mut self, outcome: Option<Status>, now: Duration, output: &mut Vec<Output>) {
    let Some(movement) = &mut self.active_action else {
      return;
    };
    if let Some(cancel) = &mut movement.cancelling {
      if outcome.is_some() {
        cancel.outcome = outcome;
      }
    } else {
      movement.cancelling = Some(Cancellation {
        deadline: now.saturating_add(STOP_TIMEOUT),
        outcome,
        reported_timeout: false,
      });
      output.push(Output::Action(Action::Cancel { id: movement.id }));
    }
  }

  fn action_result(&mut self, id: ActionId, result: ActionResult, output: &mut Vec<Output>) {
    let Some(movement) = self
      .active_action
      .as_ref()
      .filter(|movement| movement.id == id)
    else {
      return;
    };
    if let Some(cancel) = &movement.cancelling {
      if result != ActionResult::Stopped {
        return;
      }
      if let Some(outcome) = cancel.outcome {
        output.push(Output::Status(outcome));
      }
    } else if movement.target.is_none() {
      if let Some(Task::Collect {
        quantity,
        collected,
        ..
      }) = &mut self.task
      {
        if result == ActionResult::Finished {
          *collected += 1;
          output.push(Output::Status(Status::CollectionProgress {
            collected: *collected,
            quantity: *quantity,
          }));
          if collected == quantity {
            output.push(Output::Status(Status::CollectionCompleted {
              quantity: *quantity,
            }));
            self.task = None;
          }
        } else {
          output.push(Output::Status(match result {
            ActionResult::ResourceUnavailable => Status::ResourceUnavailable,
            ActionResult::InventoryFull => Status::InventoryFull,
            _ => Status::CollectionFailed,
          }));
          self.task = None;
        }
      }
    } else if result != ActionResult::Finished {
      self.task = None;
      output.push(Output::Status(if self.avoiding {
        Status::AvoidanceFailed
      } else {
        Status::NavigationFailed
      }));
      self.avoiding = false;
    }
    self.active_action = None;
  }
}

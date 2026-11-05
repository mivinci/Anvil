//! Admission and ownership contract. This does not schedule game actions.
use std::time::Duration;

use crate::{ActionId, EntityId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
  pub session: u64,
  /// Changes on every accepted goal mutation and stop, including while idle.
  pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FollowGoal {
  pub id: u64,
  pub target: EntityId,
  pub requested_by: EntityId,
  pub issued_at: Duration,
  pub expires_at: Duration,
  pub distance_blocks: u32,
  pub search_budget: Duration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
  pub observe_players: bool,
  pub navigate_without_modification: bool,
  /// Adapter guarantees Stopped means the action no longer drives controls.
  pub confirmed_cancel: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
  Following,
  Searching,
  Avoiding,
  Paused,
  Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
  StaleContext,
  InvalidGoal,
  Unsupported,
  HandoffPending,
  NoGoal,
  NotExecuting,
  StaleObservation,
  Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowSignal {
  TargetMissing,
  TargetSeen,
  Danger,
  /// Caller has confirmed safe conditions and revalidated the suspended goal.
  Safe,
  /// Search budget exhausted, path unavailable, or recovery cannot proceed.
  Blocked,
  Pause,
}

/// Validates decision provenance and exclusive ownership. Callers remain
/// responsible for terrain, permissions, action parameters and phase transitions.
pub struct GoalContract {
  context: Context,
  capabilities: Capabilities,
  goal: Option<FollowGoal>,
  phase: Phase,
  resume_phase: Phase,
  observation: Option<(u64, Duration)>,
  action: Option<ActionId>,
  cancelling: bool,
  next_action: ActionId,
}

impl GoalContract {
  pub fn new(session: u64, capabilities: Capabilities) -> Self {
    Self {
      context: Context {
        session,
        revision: 0,
      },
      capabilities,
      goal: None,
      phase: Phase::Paused,
      resume_phase: Phase::Following,
      observation: None,
      action: None,
      cancelling: false,
      next_action: 0,
    }
  }

  pub fn context(&self) -> Context {
    self.context
  }

  /// Replacement is committed only after the prior action has stopped.
  pub fn accept(
    &mut self,
    based_on: Context,
    goal: FollowGoal,
    now: Duration,
  ) -> Result<(), Rejection> {
    if based_on != self.context {
      return Err(Rejection::StaleContext);
    }
    if self.action.is_some() {
      return Err(Rejection::HandoffPending);
    }
    if goal.issued_at > now
      || goal.expires_at <= now
      || goal.distance_blocks == 0
      || goal.search_budget.is_zero()
    {
      return Err(Rejection::InvalidGoal);
    }
    let c = self.capabilities;
    if !c.observe_players || !c.navigate_without_modification || !c.confirmed_cancel {
      return Err(Rejection::Unsupported);
    }
    self.context.revision += 1;
    self.goal = Some(goal);
    self.phase = Phase::Following;
    Ok(())
  }

  /// Immediately invalidates in-flight brain/policy work. A returned action
  /// must be cancelled; no successor may start before acknowledge_stopped.
  pub fn stop(&mut self) -> Option<ActionId> {
    self.context.revision += 1;
    self.goal = None;
    self.phase = Phase::Paused;
    self.cancel_action()
  }

  pub fn cancel_action(&mut self) -> Option<ActionId> {
    if self.action.is_some() && !self.cancelling {
      self.context.revision += 1;
      self.cancelling = true;
    }
    self.action
  }

  pub fn acknowledge_stopped(&mut self, session: u64, action: ActionId) {
    if session == self.context.session && self.action == Some(action) && self.cancelling {
      self.action = None;
      self.cancelling = false;
    }
  }

  /// A racing Finished is deliberately not a cancellation acknowledgement.
  pub fn finished(&mut self, session: u64, action: ActionId) {
    if session == self.context.session && self.action == Some(action) && !self.cancelling {
      self.action = None;
    }
  }

  pub fn observe(&mut self, session: u64, sequence: u64, captured_at: Duration) {
    if session == self.context.session
      && self
        .observation
        .is_none_or(|(previous, time)| sequence > previous && captured_at >= time)
    {
      self.observation = Some((sequence, captured_at));
    }
  }

  pub fn phase(&self) -> Phase {
    self.phase
  }

  /// Changes invalidate pending policy results via the revision. Any active
  /// action is cancelled before a successor can own the controls.
  pub fn signal(&mut self, signal: FollowSignal) -> Option<ActionId> {
    if self.goal.is_none() {
      return None;
    }
    let next = match (self.phase, signal) {
      (Phase::Following, FollowSignal::TargetMissing) => Phase::Searching,
      (Phase::Searching, FollowSignal::TargetSeen) => Phase::Following,
      (Phase::Following | Phase::Searching, FollowSignal::Danger) => {
        self.resume_phase = self.phase;
        Phase::Avoiding
      }
      (Phase::Avoiding, FollowSignal::Safe) => self.resume_phase,
      (Phase::Following | Phase::Searching | Phase::Avoiding, FollowSignal::Blocked) => {
        Phase::Blocked
      }
      (_, FollowSignal::Pause) => Phase::Paused,
      _ => return None,
    };
    if next == self.phase {
      return None;
    }
    self.phase = next;
    self.context.revision += 1;
    self.cancel_action()
  }

  /// Reserves an ID only. Caller must validate action parameters first.
  pub fn reserve_action(
    &mut self,
    based_on: Context,
    observation: u64,
    now: Duration,
    max_age: Duration,
  ) -> Result<ActionId, Rejection> {
    if based_on != self.context {
      return Err(Rejection::StaleContext);
    }
    let goal = self.goal.as_ref().ok_or(Rejection::NoGoal)?;
    if now >= goal.expires_at {
      return Err(Rejection::Expired);
    }
    if !matches!(
      self.phase,
      Phase::Following | Phase::Searching | Phase::Avoiding
    ) {
      return Err(Rejection::NotExecuting);
    }
    let (seq, captured_at) = self.observation.ok_or(Rejection::StaleObservation)?;
    if seq != observation || now < captured_at || now - captured_at >= max_age {
      return Err(Rejection::StaleObservation);
    }
    if self.action.is_some() {
      return Err(Rejection::HandoffPending);
    }
    self.next_action += 1;
    self.action = Some(self.next_action);
    Ok(self.next_action)
  }
}

use anvil_agent::{
  Action, ActionResult, Command, Event, Executor, Observation, Output, Position, Status,
};
use std::time::Duration;

fn step(executor: &mut Executor, event: Event, millis: u64) -> Vec<Output> {
  executor.step(event, Duration::from_millis(millis))
}

fn follow() -> Event {
  Event::Command(Command::Follow {
    target: 42,
    distance: 2,
  })
}

fn observe(tick: u64, target: Option<i32>) -> Event {
  Event::Observation(Observation {
    tick,
    position: Position(0, 64, 0),
    ignited_creepers: vec![],
    nearby_players: target
      .into_iter()
      .map(|x| (42, Position(x, 64, 0)))
      .collect(),
  })
}

fn result(id: u64, result: ActionResult) -> Event {
  Event::ActionResult { id, result }
}

#[test]
fn follows_a_moving_target_and_stops_without_restarting() {
  let mut executor = Executor::default();
  assert!(step(&mut executor, follow(), 0).is_empty());
  assert_eq!(
    step(&mut executor, observe(1, Some(10)), 50),
    [Output::Action(Action::Navigate {
      id: 1,
      target: Position(10, 64, 0),
      distance: 2
    })]
  );
  assert!(step(&mut executor, observe(2, Some(11)), 100).is_empty());
  assert_eq!(
    step(&mut executor, observe(3, Some(14)), 150),
    [Output::Action(Action::Cancel { id: 1 })]
  );
  assert!(step(&mut executor, result(1, ActionResult::Stopped), 160).is_empty());
  assert_eq!(
    step(&mut executor, observe(4, Some(14)), 200),
    [Output::Action(Action::Navigate {
      id: 2,
      target: Position(14, 64, 0),
      distance: 2
    })]
  );
  assert_eq!(
    step(&mut executor, Event::Command(Command::Stop), 210),
    [Output::Action(Action::Cancel { id: 2 })]
  );
  assert!(step(&mut executor, result(1, ActionResult::Stopped), 220).is_empty());
  assert!(step(&mut executor, result(2, ActionResult::Finished), 230).is_empty());
  assert_eq!(
    step(&mut executor, follow(), 240),
    [Output::Status(Status::Busy)]
  );
  assert_eq!(
    step(&mut executor, result(2, ActionResult::Stopped), 250),
    [Output::Status(Status::Stopped)]
  );
  assert!(step(&mut executor, observe(5, Some(20)), 300).is_empty());
}

#[test]
fn stop_timeout_blocks_new_actions_until_session_is_torn_down() {
  let mut executor = Executor::default();
  step(&mut executor, follow(), 0);
  step(&mut executor, observe(1, Some(10)), 50);
  step(&mut executor, Event::Command(Command::Stop), 100);
  assert_eq!(
    step(&mut executor, Event::Timer, 2100),
    [Output::Status(Status::StopUnconfirmed)]
  );
  assert_eq!(
    step(&mut executor, follow(), 2200),
    [Output::Status(Status::Busy)]
  );
  assert!(step(&mut executor, observe(2, Some(20)), 2250).is_empty());
  assert_eq!(
    step(&mut executor, Event::Disconnected, 2300),
    [Output::Status(Status::Disconnected)]
  );
  assert!(step(&mut executor, follow(), 2400).is_empty());
  assert!(matches!(
    step(&mut executor, observe(1, Some(10)), 2450).as_slice(),
    [Output::Action(Action::Navigate { id: 2, .. })]
  ));
}

#[test]
fn losing_the_target_cancels_movement_and_reports_after_grace_period() {
  let mut executor = Executor::default();
  step(&mut executor, follow(), 0);
  step(&mut executor, observe(1, Some(10)), 50);
  assert_eq!(
    step(&mut executor, observe(2, None), 100),
    [Output::Action(Action::Cancel { id: 1 })]
  );
  assert!(step(&mut executor, result(1, ActionResult::Stopped), 150).is_empty());
  for (tick, millis) in [(3, 900), (4, 1700), (5, 2500)] {
    assert!(step(&mut executor, observe(tick, None), millis).is_empty());
  }
  assert_eq!(
    step(&mut executor, Event::Timer, 3050),
    [Output::Status(Status::TargetLost)]
  );
  assert!(step(&mut executor, observe(6, Some(10)), 3100).is_empty());
}

fn danger(tick: u64) -> Event {
  Event::Observation(Observation {
    tick,
    position: Position(0, 64, 0),
    ignited_creepers: vec![Position(3, 64, 0)],
    nearby_players: vec![],
  })
}

fn start_retreat(executor: &mut Executor) {
  step(executor, follow(), 0);
  step(executor, observe(1, Some(10)), 50);
  assert_eq!(
    step(executor, danger(2), 100),
    [
      Output::Status(Status::AvoidanceStarted),
      Output::Action(Action::Cancel { id: 1 })
    ]
  );
  assert!(step(executor, result(1, ActionResult::Finished), 110).is_empty());
  assert!(step(executor, danger(3), 120).is_empty());
  assert!(step(executor, result(1, ActionResult::Stopped), 130).is_empty());
  assert_eq!(
    step(executor, danger(4), 140),
    [Output::Action(Action::Navigate {
      id: 2,
      target: Position(-8, 64, 0),
      distance: 0
    })]
  );
}

#[test]
fn retreat_preempts_follow_and_resumes_using_a_fresh_target() {
  let mut executor = Executor::default();
  start_retreat(&mut executor);
  assert!(step(&mut executor, danger(5), 200).is_empty());
  assert!(step(&mut executor, observe(6, Some(20)), 250).is_empty());
  assert!(step(&mut executor, danger(7), 300).is_empty());
  assert!(step(&mut executor, observe(8, Some(20)), 350).is_empty());
  assert_eq!(
    step(&mut executor, observe(9, Some(20)), 850),
    [
      Output::Status(Status::AvoidanceEnded),
      Output::Action(Action::Cancel { id: 2 })
    ]
  );
  assert!(step(&mut executor, result(2, ActionResult::Stopped), 900).is_empty());
  assert_eq!(
    step(&mut executor, observe(10, Some(25)), 950),
    [Output::Action(Action::Navigate {
      id: 3,
      target: Position(25, 64, 0),
      distance: 2
    })]
  );
}

#[test]
fn manual_stop_cancels_retreat_and_discards_suspended_follow() {
  let mut executor = Executor::default();
  start_retreat(&mut executor);
  assert_eq!(
    step(&mut executor, Event::Command(Command::Stop), 150),
    [Output::Action(Action::Cancel { id: 2 })]
  );
  assert!(step(&mut executor, danger(5), 160).is_empty());
  assert_eq!(
    step(&mut executor, result(2, ActionResult::Stopped), 170),
    [Output::Status(Status::Stopped)]
  );
  assert!(step(&mut executor, observe(6, Some(20)), 180).is_empty());
}

#[test]
fn duplicate_observations_do_not_keep_movement_alive() {
  let mut executor = Executor::default();
  step(&mut executor, follow(), 0);
  step(&mut executor, observe(1, Some(10)), 50);
  assert!(step(&mut executor, observe(1, Some(10)), 900).is_empty());
  assert_eq!(
    step(&mut executor, Event::Timer, 1050),
    [Output::Action(Action::Cancel { id: 1 })]
  );
  assert!(step(&mut executor, observe(2, Some(10)), 1100).is_empty());
  assert_eq!(
    step(&mut executor, result(1, ActionResult::Stopped), 1150),
    [Output::Status(Status::ObservationExpired)]
  );
}

#[test]
fn retreat_with_fresh_observations_but_no_movement_fails_and_drops_follow() {
  let mut executor = Executor::default();
  start_retreat(&mut executor);
  for (tick, millis) in [(5, 900), (6, 1700), (7, 2500)] {
    assert!(step(&mut executor, danger(tick), millis).is_empty());
  }
  assert_eq!(
    step(&mut executor, danger(8), 3140),
    [Output::Action(Action::Cancel { id: 2 })]
  );
  assert_eq!(
    step(&mut executor, result(2, ActionResult::Stopped), 3150),
    [Output::Status(Status::AvoidanceFailed)]
  );
  assert!(step(&mut executor, observe(9, Some(10)), 3200).is_empty());
}

#[test]
fn collection_counts_each_confirmed_pickup_once() {
  use anvil_agent::Resource;
  let mut executor = Executor::default();
  step(
    &mut executor,
    Event::Command(Command::Collect {
      resource: Resource::OakLog,
      quantity: 2,
    }),
    0,
  );
  assert_eq!(
    step(&mut executor, observe(1, None), 50),
    [Output::Action(Action::CollectOne {
      id: 1,
      resource: Resource::OakLog
    })]
  );
  assert_eq!(
    step(&mut executor, result(1, ActionResult::Finished), 60),
    [Output::Status(Status::CollectionProgress {
      collected: 1,
      quantity: 2
    })]
  );
  assert!(step(&mut executor, result(1, ActionResult::Finished), 65).is_empty());
  assert!(matches!(
    step(&mut executor, observe(2, None), 70).as_slice(),
    [Output::Action(Action::CollectOne { id: 2, .. })]
  ));
  assert_eq!(
    step(&mut executor, result(2, ActionResult::Finished), 80),
    [
      Output::Status(Status::CollectionProgress {
        collected: 2,
        quantity: 2
      }),
      Output::Status(Status::CollectionCompleted { quantity: 2 }),
    ]
  );
  assert!(step(&mut executor, observe(3, None), 90).is_empty());
}

#[test]
fn collection_failure_and_cancellation_share_action_lifecycle() {
  use anvil_agent::Resource;
  let collect = || {
    Event::Command(Command::Collect {
      resource: Resource::OakLog,
      quantity: 1,
    })
  };
  let mut executor = Executor::default();
  step(&mut executor, collect(), 0);
  step(&mut executor, observe(1, None), 50);
  assert_eq!(
    step(&mut executor, result(1, ActionResult::InventoryFull), 60),
    [Output::Status(Status::InventoryFull)]
  );
  step(&mut executor, collect(), 70);
  step(&mut executor, observe(2, None), 80);
  assert_eq!(
    step(&mut executor, Event::Command(Command::Stop), 90),
    [Output::Action(Action::Cancel { id: 2 })]
  );
  assert!(step(&mut executor, result(2, ActionResult::Finished), 100).is_empty());
  assert_eq!(
    step(&mut executor, result(2, ActionResult::Stopped), 110),
    [Output::Status(Status::Stopped)]
  );
  assert!(step(&mut executor, observe(3, None), 120).is_empty());
}

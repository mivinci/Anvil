use anvil_agent::contract::{
  Capabilities, FollowGoal, FollowSignal, GoalContract, Phase, Rejection,
};
use std::time::Duration;

fn time(ms: u64) -> Duration {
  Duration::from_millis(ms)
}

fn goal(id: u64) -> FollowGoal {
  FollowGoal {
    id,
    target: 42,
    requested_by: 7,
    issued_at: time(0),
    expires_at: time(10000),
    distance_blocks: 2,
    search_budget: time(3000),
  }
}

fn ready() -> GoalContract {
  let mut c = GoalContract::new(
    1,
    Capabilities {
      observe_players: true,
      navigate_without_modification: true,
      confirmed_cancel: true,
    },
  );
  c.accept(c.context(), goal(1), time(0)).unwrap();
  c.observe(1, 1, time(10));
  c
}

#[test]
fn delayed_brain_does_not_stop_follow_and_cannot_override_a_later_stop() {
  let mut c = ready();
  let brain_context = c.context();
  let action = c
    .reserve_action(c.context(), 1, time(20), time(100))
    .unwrap();
  c.finished(1, action);
  c.observe(1, 2, time(200));
  let next = c
    .reserve_action(c.context(), 2, time(210), time(100))
    .unwrap();
  assert_eq!(c.stop(), Some(next));
  c.acknowledge_stopped(1, next);
  assert_eq!(
    c.accept(brain_context, goal(2), time(220)),
    Err(Rejection::StaleContext)
  );
  assert_eq!(
    c.reserve_action(c.context(), 2, time(220), time(100)),
    Err(Rejection::NoGoal)
  );
}

#[test]
fn replacing_goal_waits_for_matching_cancel_ack_and_rejects_old_policy() {
  let mut c = ready();
  let old = c.context();
  let a = c.reserve_action(old, 1, time(20), time(100)).unwrap();
  assert_eq!(
    c.accept(old, goal(2), time(30)),
    Err(Rejection::HandoffPending)
  );
  assert_eq!(c.cancel_action(), Some(a));
  c.finished(1, a);
  c.acknowledge_stopped(2, a);
  c.acknowledge_stopped(1, a + 1);
  assert_eq!(
    c.accept(c.context(), goal(2), time(40)),
    Err(Rejection::HandoffPending)
  );
  c.acknowledge_stopped(1, a);
  c.accept(c.context(), goal(2), time(50)).unwrap();
  assert_eq!(
    c.reserve_action(old, 1, time(60), time(100)),
    Err(Rejection::StaleContext)
  );
}

#[test]
fn stale_observations_expiry_and_missing_capabilities_prevent_actions() {
  let mut c = ready();
  c.observe(2, 99, time(50));
  c.observe(1, 1, time(60));
  assert_eq!(
    c.reserve_action(c.context(), 1, time(110), time(100)),
    Err(Rejection::StaleObservation)
  );
  c.observe(1, 2, time(120));
  assert_eq!(
    c.reserve_action(c.context(), 1, time(130), time(100)),
    Err(Rejection::StaleObservation)
  );
  assert_eq!(
    c.reserve_action(c.context(), 2, time(10000), time(100)),
    Err(Rejection::Expired)
  );
  let mut unsupported = GoalContract::new(1, Capabilities::default());
  assert_eq!(
    unsupported.accept(unsupported.context(), goal(1), time(0)),
    Err(Rejection::Unsupported)
  );
}

#[test]
fn search_avoidance_recovery_and_blocking_keep_the_continuous_goal() {
  let mut c = ready();
  c.signal(FollowSignal::TargetMissing);
  assert_eq!(c.phase(), Phase::Searching);
  c.signal(FollowSignal::Danger);
  assert_eq!(c.phase(), Phase::Avoiding);
  c.signal(FollowSignal::Safe);
  assert_eq!(c.phase(), Phase::Searching);
  c.signal(FollowSignal::TargetSeen);
  assert_eq!(c.phase(), Phase::Following);
  c.signal(FollowSignal::Blocked);
  c.signal(FollowSignal::TargetSeen);
  assert_eq!(c.phase(), Phase::Blocked);
  assert_eq!(
    c.reserve_action(c.context(), 1, time(20), time(100)),
    Err(Rejection::NotExecuting)
  );
  c.accept(c.context(), goal(1), time(30)).unwrap();
  c.signal(FollowSignal::Pause);
  c.signal(FollowSignal::Safe);
  assert_eq!(c.phase(), Phase::Paused);
}

#[test]
fn phase_change_cancels_action_and_invalidates_inflight_policy() {
  let mut c = ready();
  let context = c.context();
  let a = c.reserve_action(context, 1, time(20), time(100)).unwrap();
  assert_eq!(c.signal(FollowSignal::Danger), Some(a));
  assert_eq!(
    c.reserve_action(context, 1, time(30), time(100)),
    Err(Rejection::StaleContext)
  );
  assert_eq!(
    c.reserve_action(c.context(), 1, time(30), time(100)),
    Err(Rejection::HandoffPending)
  );
  c.stop();
  c.acknowledge_stopped(1, a);
  c.signal(FollowSignal::Safe);
  assert_eq!(
    c.reserve_action(c.context(), 1, time(40), time(100)),
    Err(Rejection::NoGoal)
  );
}

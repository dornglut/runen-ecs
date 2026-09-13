use runen_ecs::prelude::*;
use runen_ecs::system::OrderingDirection;
use runen_ecs::{RuntimeError, ScheduleValidationError};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Copy, Clone)]
struct TargetA;
impl SystemSet for TargetA {
    fn name() -> &'static str {
        "TargetA"
    }
}

#[derive(Copy, Clone)]
struct TargetB;
impl SystemSet for TargetB {
    fn name() -> &'static str {
        "TargetB"
    }
}

#[derive(Copy, Clone)]
struct CycleA;
impl SystemSet for CycleA {
    fn name() -> &'static str {
        "CycleA"
    }
}

#[derive(Copy, Clone)]
struct CycleB;
impl SystemSet for CycleB {
    fn name() -> &'static str {
        "CycleB"
    }
}

#[derive(Debug, Default, Resource)]
struct Order(Vec<&'static str>);

fn source() {}
fn other() {}
fn cycle_a() {}
fn cycle_b() {}

fn record_source(mut order: ResMut<Order>) {
    order.0.push("source");
}

fn record_target(mut order: ResMut<Order>) {
    order.0.push("target");
}

fn schedule_error(error: RuntimeError) -> ScheduleValidationError {
    match error {
        RuntimeError::Schedule(error) => error,
        other => panic!("expected schedule validation error, got {other:#}"),
    }
}

#[test]
fn required_missing_before_target_is_structured_error() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, source.before(TargetA));

    let error = schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err());
    match error {
        ScheduleValidationError::UnresolvedOrderingReference {
            schedule,
            source_system,
            direction,
            target_set,
        } => {
            assert_eq!(schedule, "Update");
            assert!(source_system.name().ends_with("source"));
            assert_eq!(source_system.same_name_occurrence(), 1);
            assert_eq!(direction, OrderingDirection::Before);
            assert_eq!(target_set.name(), "TargetA");
        }
        other => panic!("expected unresolved ordering reference, got {other:?}"),
    }
}

#[test]
fn required_missing_after_target_is_structured_error() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, source.after(TargetA));

    let error = schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err());
    match error {
        ScheduleValidationError::UnresolvedOrderingReference {
            direction,
            target_set,
            ..
        } => {
            assert_eq!(direction, OrderingDirection::After);
            assert_eq!(target_set.name(), "TargetA");
        }
        other => panic!("expected unresolved ordering reference, got {other:?}"),
    }
}

#[test]
fn self_membership_does_not_satisfy_required_target() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        source.in_set(TargetA).before(TargetA),
    );

    let error = schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err());
    assert!(matches!(
        error,
        ScheduleValidationError::UnresolvedOrderingReference {
            direction: OrderingDirection::Before,
            ..
        }
    ));
}

#[test]
fn optional_missing_target_is_accepted_and_repetition_is_idempotent() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        source
            .before_if_present(TargetA)
            .before_if_present(TargetA),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
}

#[test]
fn optional_present_target_has_normal_precedence() {
    let mut world = World::new();
    world.insert_resource(Order::default());
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, record_target.in_set(TargetA));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        record_source.before_if_present(TargetA),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Order>().unwrap().0, vec!["source", "target"]);
}

#[test]
fn required_dominates_optional_independent_of_builder_call_order() {
    let first = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        runtime.add_systems::<Update, _, _>(
            &mut world,
            source.before_if_present(TargetA).before(TargetA),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    let second = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        runtime.add_systems::<Update, _, _>(
            &mut world,
            source.before(TargetA).before_if_present(TargetA),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    assert_eq!(first, second);
}

#[test]
fn unresolved_selection_is_deterministic_under_builder_permutation() {
    let first = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        runtime.add_systems::<Update, _, _>(
            &mut world,
            source.after(TargetA).before(TargetB),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    let second = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        runtime.add_systems::<Update, _, _>(
            &mut world,
            source.before(TargetB).after(TargetA),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    assert_eq!(first, second);
    assert!(matches!(
        first,
        ScheduleValidationError::UnresolvedOrderingReference {
            direction: OrderingDirection::Before,
            ..
        }
    ));
}

#[test]
fn unresolved_reference_is_reported_before_cycle_derivation() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, source.before(TargetA));
    runtime.add_systems::<Update, _, _>(
        &mut world,
        cycle_a.in_set(CycleA).after(CycleB),
    );
    runtime.add_systems::<Update, _, _>(
        &mut world,
        cycle_b.in_set(CycleB).after(CycleA),
    );
    runtime.add_systems::<Update, _, _>(&mut world, other);

    let error = schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err());
    assert!(matches!(
        error,
        ScheduleValidationError::UnresolvedOrderingReference { .. }
    ));
}

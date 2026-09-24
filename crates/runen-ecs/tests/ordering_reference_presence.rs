use runen_ecs::prelude::*;
use runen_ecs::system::{
    OrderingDirection, ScheduleDiagnosticDescriptor, SystemSetDiagnosticDescriptor, SystemSetKey,
};
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
    fn name(&self) -> &'static str {
        "TargetA"
    }
}

#[derive(Copy, Clone)]
struct TargetB;
impl SystemSet for TargetB {
    fn name(&self) -> &'static str {
        "TargetB"
    }
}

#[derive(Copy, Clone)]
struct CycleA;
impl SystemSet for CycleA {
    fn name(&self) -> &'static str {
        "CycleA"
    }
}

#[derive(Copy, Clone)]
struct CycleB;
impl SystemSet for CycleB {
    fn name(&self) -> &'static str {
        "CycleB"
    }
}

#[derive(Copy, Clone)]
struct CollidingTargetA;
impl SystemSet for CollidingTargetA {
    fn name(&self) -> &'static str {
        "SharedTarget"
    }
}

#[derive(Copy, Clone)]
struct CollidingTargetB;
impl SystemSet for CollidingTargetB {
    fn name(&self) -> &'static str {
        "SharedTarget"
    }
}

#[derive(Copy, Clone)]
struct CollidingScheduleA;
impl ScheduleLabel for CollidingScheduleA {
    fn name() -> &'static str {
        "SharedSchedule"
    }
}

#[derive(Copy, Clone)]
struct CollidingScheduleB;
impl ScheduleLabel for CollidingScheduleB {
    fn name() -> &'static str {
        "SharedSchedule"
    }
}

#[derive(Copy, Clone)]
struct TargetAliasOptional;
impl SystemSet for TargetAliasOptional {
    fn key(&self) -> SystemSetKey {
        CollidingTargetA.key()
    }
}

#[derive(Copy, Clone)]
struct TargetAliasRequired;
impl SystemSet for TargetAliasRequired {
    fn key(&self) -> SystemSetKey {
        CollidingTargetA.key()
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

fn unresolved_schedule(error: &ScheduleValidationError) -> ScheduleDiagnosticDescriptor {
    match error {
        ScheduleValidationError::UnresolvedOrderingReference { schedule, .. } => *schedule,
        other => panic!("expected unresolved ordering reference, got {other:?}"),
    }
}

fn unresolved_target(error: &ScheduleValidationError) -> SystemSetDiagnosticDescriptor {
    match error {
        ScheduleValidationError::UnresolvedOrderingReference { target_set, .. } => *target_set,
        other => panic!("expected unresolved ordering reference, got {other:?}"),
    }
}

#[test]
fn required_missing_before_target_is_structured_error() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, source.before(TargetA));

    let error = schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err());
    match error {
        ScheduleValidationError::UnresolvedOrderingReference {
            schedule,
            source_system,
            direction,
            target_set,
        } => {
            assert_eq!(schedule.name(), "Update");
            assert!(schedule.type_name().ends_with("::Update"));
            assert_eq!(schedule.same_name_and_type_occurrence(), 1);
            assert!(source_system.name().ends_with("source"));
            assert_eq!(source_system.same_name_occurrence(), 1);
            assert_eq!(direction, OrderingDirection::Before);
            assert_eq!(target_set.name(), "TargetA");
            assert!(target_set.type_name().ends_with("::TargetA"));
            assert_eq!(target_set.same_name_and_type_occurrence(), 1);
        }
        other => panic!("expected unresolved ordering reference, got {other:?}"),
    }
}

#[test]
fn required_missing_after_target_is_structured_error() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, source.after(TargetA));

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
    let _ = runtime.add_systems(Update, source.in_set(TargetA).before(TargetA));

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
    let _ = runtime.add_systems(
        Update,
        source.before_if_present(TargetA).before_if_present(TargetA),
    );

    runtime.run_schedule::<Update>(&mut world).unwrap();
}

#[test]
fn optional_present_target_has_normal_precedence() {
    let mut world = World::new();
    world.insert_resource(Order::default());
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, record_target.in_set(TargetA));
    let _ = runtime.add_systems(Update, record_source.before_if_present(TargetA));

    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(
        world.resource::<Order>().unwrap().0,
        vec!["source", "target"]
    );
}

#[test]
fn required_dominates_optional_independent_of_builder_call_order() {
    let first = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(Update, source.before_if_present(TargetA).before(TargetA));
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    let second = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(Update, source.before(TargetA).before_if_present(TargetA));
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    assert_eq!(first, second);
}

#[test]
fn unresolved_selection_is_deterministic_under_builder_permutation() {
    let first = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(Update, source.after(TargetA).before(TargetB));
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };

    let second = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(Update, source.before(TargetB).after(TargetA));
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
fn colliding_target_labels_are_distinct_and_builder_order_independent() {
    let first = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            Update,
            source.before(CollidingTargetB).before(CollidingTargetA),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };
    let second = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            Update,
            source.before(CollidingTargetA).before(CollidingTargetB),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };
    assert_eq!(first, second);

    let only_a = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(Update, source.before(CollidingTargetA));
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };
    let only_b = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(Update, source.before(CollidingTargetB));
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };
    let descriptor_a = unresolved_target(&only_a);
    let descriptor_b = unresolved_target(&only_b);
    assert_eq!(descriptor_a.name(), "SharedTarget");
    assert_eq!(descriptor_b.name(), "SharedTarget");
    assert_ne!(descriptor_a, descriptor_b);
    assert_ne!(descriptor_a.type_name(), descriptor_b.type_name());
}

#[test]
fn colliding_schedule_labels_are_distinct() {
    let schedule_a_error = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(CollidingScheduleA, source.before(TargetA));
        let _ = runtime.add_systems(CollidingScheduleB, other);
        schedule_error(
            runtime
                .run_schedule::<CollidingScheduleA>(&mut world)
                .unwrap_err(),
        )
    };
    let schedule_b_error = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(CollidingScheduleB, source.before(TargetA));
        let _ = runtime.add_systems(CollidingScheduleA, other);
        schedule_error(
            runtime
                .run_schedule::<CollidingScheduleB>(&mut world)
                .unwrap_err(),
        )
    };

    let descriptor_a = unresolved_schedule(&schedule_a_error);
    let descriptor_b = unresolved_schedule(&schedule_b_error);
    assert_eq!(descriptor_a.name(), "SharedSchedule");
    assert_eq!(descriptor_b.name(), "SharedSchedule");
    assert_ne!(descriptor_a, descriptor_b);
    assert_ne!(descriptor_a.type_name(), descriptor_b.type_name());
}

#[test]
fn diagnostic_identity_follows_returned_semantic_keys_not_wrapper_types() {
    let set_error = {
        let mut world = World::new();
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            Update,
            source
                .before_if_present(TargetAliasOptional)
                .before(TargetAliasRequired),
        );
        schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err())
    };
    let target = unresolved_target(&set_error);
    assert_eq!(target.name(), "SharedTarget");
    assert!(target.type_name().ends_with("::CollidingTargetA"));
}

#[test]
fn unresolved_reference_is_reported_before_cycle_derivation() {
    let mut world = World::new();
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, source.before(TargetA));
    let _ = runtime.add_systems(Update, cycle_a.in_set(CycleA).after(CycleB));
    let _ = runtime.add_systems(Update, cycle_b.in_set(CycleB).after(CycleA));
    let _ = runtime.add_systems(Update, other);

    let error = schedule_error(runtime.run_schedule::<Update>(&mut world).unwrap_err());
    assert!(matches!(
        error,
        ScheduleValidationError::UnresolvedOrderingReference { .. }
    ));
}

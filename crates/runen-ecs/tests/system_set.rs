use std::any::TypeId;

use runen_ecs::prelude::*;
use runen_ecs::system::{OrderingDirection, OrderingPresence, ScheduleOrderingResolutionKind};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {}

#[derive(Copy, Clone, SystemSet)]
enum Phase {
    Prepare,
    Simulate,
    Optional,
}

fn prepare_system() {}
fn simulate_system() {}
fn before_system() {}
fn after_system() {}
fn optional_before_system() {}
fn optional_after_system() {}

#[derive(Copy, Clone, SystemSet)]
struct DerivedPrepare;

#[derive(Copy, Clone)]
struct ManualNamedSet;

impl SystemSet for ManualNamedSet {
    fn name(&self) -> &'static str {
        "ManualPrepare"
    }
}

#[derive(Copy, Clone)]
struct DefaultMarker;

impl SystemSet for DefaultMarker {}

#[test]
fn derived_keys_are_value_aware_and_stable() {
    let prepare = Phase::Prepare.key();
    let simulate = Phase::Simulate.key();

    assert_eq!(prepare.name(), "Phase::Prepare");
    assert_eq!(simulate.name(), "Phase::Simulate");
    assert_eq!(prepare.type_id(), TypeId::of::<Phase>());
    assert_eq!(simulate.type_id(), TypeId::of::<Phase>());
    assert_ne!(prepare, simulate);
    assert_eq!(prepare, Phase::Prepare.key());
}

#[test]
fn unit_markers_keep_type_based_identity_and_manual_names() {
    let default_marker = DefaultMarker.key();
    assert_eq!(
        default_marker.name(),
        std::any::type_name::<DefaultMarker>()
    );
    assert_eq!(default_marker.type_id(), TypeId::of::<DefaultMarker>());

    let derived = DerivedPrepare.key();
    assert_eq!(derived.name(), std::any::type_name::<DerivedPrepare>());
    assert_eq!(derived.type_id(), TypeId::of::<DerivedPrepare>());

    let manual = ManualNamedSet.key();
    assert_eq!(manual.name(), "ManualPrepare");
    assert_eq!(manual.type_id(), TypeId::of::<ManualNamedSet>());
}

#[test]
fn derived_keys_drive_all_system_configuration_ordering_forms() {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, prepare_system.in_set(Phase::Prepare))
        .unwrap();
    runtime
        .add_systems(Update, simulate_system.in_set(Phase::Simulate))
        .unwrap();
    runtime
        .add_systems(
            Update,
            (
                before_system.before(Phase::Simulate),
                after_system.after(Phase::Prepare),
                optional_before_system.before_if_present(Phase::Optional),
                optional_after_system.after_if_present(Phase::Prepare),
            ),
        )
        .unwrap();

    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    assert_eq!(inspection.ordering_resolutions().len(), 4);

    let simulate_before = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.source().name().ends_with("::before_system"))
        .unwrap();
    assert_eq!(simulate_before.direction(), OrderingDirection::Before);
    assert_eq!(simulate_before.presence(), OrderingPresence::Required);
    assert_eq!(simulate_before.target_set().name(), "Phase::Simulate");
    assert!(matches!(
        simulate_before.kind(),
        ScheduleOrderingResolutionKind::Resolved { target_systems } if target_systems.len() == 1
    ));

    let prepare_after = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.source().name().ends_with("::after_system"))
        .unwrap();
    assert_eq!(prepare_after.direction(), OrderingDirection::After);
    assert_eq!(prepare_after.presence(), OrderingPresence::Required);
    assert_eq!(prepare_after.target_set().name(), "Phase::Prepare");
    assert!(matches!(
        prepare_after.kind(),
        ScheduleOrderingResolutionKind::Resolved { target_systems } if target_systems.len() == 1
    ));

    let optional_before = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| {
            resolution
                .source()
                .name()
                .ends_with("::optional_before_system")
        })
        .unwrap();
    assert_eq!(optional_before.direction(), OrderingDirection::Before);
    assert_eq!(optional_before.presence(), OrderingPresence::Optional);
    assert_eq!(optional_before.target_set().name(), "Phase::Optional");
    assert!(matches!(
        optional_before.kind(),
        ScheduleOrderingResolutionKind::AbsentOptional
    ));

    let optional_after = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| {
            resolution
                .source()
                .name()
                .ends_with("::optional_after_system")
        })
        .unwrap();
    assert_eq!(optional_after.direction(), OrderingDirection::After);
    assert_eq!(optional_after.presence(), OrderingPresence::Optional);
    assert_eq!(optional_after.target_set().name(), "Phase::Prepare");
    assert!(matches!(
        optional_after.kind(),
        ScheduleOrderingResolutionKind::Resolved { target_systems } if target_systems.len() == 1
    ));
}

#[test]
fn system_set_derive_rejects_non_supported_inputs() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/system_set_payload.rs");
    cases.compile_fail("tests/ui/system_set_named_struct.rs");
    cases.compile_fail("tests/ui/system_set_tuple_struct.rs");
    cases.compile_fail("tests/ui/system_set_union.rs");
}

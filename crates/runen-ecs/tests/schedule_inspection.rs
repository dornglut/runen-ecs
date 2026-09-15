use runen_ecs::LocalCommands;
use runen_ecs::prelude::*;
use runen_ecs::system::{
    OrderingDirection, OrderingPresence, ScheduleAccessConflictKind, ScheduleOrderingCycle,
    ScheduleOrderingResolutionKind, SchedulePublicationObligation,
};
use runen_ecs::{ExecutionMobility, RuntimeError, ScheduleValidationError};

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {}

#[derive(Copy, Clone)]
struct Unused;
impl ScheduleLabel for Unused {}

#[derive(Copy, Clone)]
struct Alpha;
impl SystemSet for Alpha {}

#[derive(Copy, Clone)]
struct Beta;
impl SystemSet for Beta {}

#[derive(Copy, Clone)]
struct Gamma;
impl SystemSet for Gamma {}

#[derive(Default, Resource)]
struct Counter(u32);

#[derive(Resource)]
struct Shared(u32);

fn source() {}
fn target() {}
fn middle() {}
fn cycle_a() {}
fn cycle_b() {}
fn cycle_c() {}
fn mobility_target() {}
fn increments(mut counter: ResMut<Counter>) {
    counter.0 = counter.0.saturating_add(1);
}
fn write_left(mut shared: ResMut<Shared>) {
    shared.0 = shared.0.saturating_add(1);
}
fn write_right(mut shared: ResMut<Shared>) {
    shared.0 = shared.0.saturating_add(1);
}
fn deferred_producer(_commands: LocalCommands) {}
fn unrelated_deferred(_commands: LocalCommands) {}

fn descriptor(
    inspection: &runen_ecs::ScheduleInspection,
    suffix: &str,
) -> runen_ecs::system::SystemDiagnosticDescriptor {
    inspection
        .systems()
        .iter()
        .find(|descriptor| descriptor.name().ends_with(suffix))
        .cloned()
        .unwrap_or_else(|| panic!("missing system descriptor ending in {suffix}"))
}

fn schedule_error(error: RuntimeError) -> ScheduleValidationError {
    match error {
        RuntimeError::Schedule(error) => error,
        other => panic!("expected schedule error, got {other:?}"),
    }
}

#[test]
fn inspection_projects_required_optional_and_absent_resolutions() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, target.in_set(Alpha));
    let _ = runtime.add_systems(Update, source.before(Alpha).before_if_present(Beta));

    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let source_descriptor = descriptor(&inspection, "::source");
    let target_descriptor = descriptor(&inspection, "::target");
    let alpha = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.target_set().name() == Alpha.name())
        .unwrap();
    assert_eq!(alpha.source(), &source_descriptor);
    assert_eq!(alpha.direction(), OrderingDirection::Before);
    assert_eq!(alpha.presence(), OrderingPresence::Required);
    assert!(matches!(
        alpha.kind(),
        ScheduleOrderingResolutionKind::Resolved { target_systems }
            if target_systems == &vec![target_descriptor.clone()]
    ));

    let beta = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.target_set().name() == Beta.name())
        .unwrap();
    assert_eq!(beta.presence(), OrderingPresence::Optional);
    assert!(matches!(
        beta.kind(),
        ScheduleOrderingResolutionKind::AbsentOptional
    ));
    assert_eq!(inspection.precedence_edges().len(), 1);
    assert!(
        inspection
            .precedence_edges()
            .iter()
            .any(|edge| edge.predecessor() == &source_descriptor
                && edge.successor() == &target_descriptor)
    );
}

#[test]
fn one_edge_retains_multiple_normalized_reasons() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, source.before(Alpha).before_if_present(Beta));
    let _ = runtime.add_systems(Update, target.in_set(Alpha).in_set(Beta));

    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let edge = inspection
        .precedence_edges()
        .iter()
        .find(|edge| edge.predecessor().name().ends_with("::source"))
        .unwrap();
    assert_eq!(edge.reasons().len(), 2);
    assert!(
        edge.reasons()
            .iter()
            .any(|reason| reason.presence() == OrderingPresence::Required)
    );
    assert!(
        edge.reasons()
            .iter()
            .any(|reason| reason.presence() == OrderingPresence::Optional)
    );
    let path = inspection
        .precedence_path(edge.predecessor(), edge.successor())
        .unwrap();
    assert_eq!(path.edges().len(), 1);
    assert_eq!(path.edges()[0].reasons().len(), 2);
}

#[test]
fn transitive_precedence_is_reason_carrying_and_suppresses_ambiguity() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, write_left.in_set(Alpha));
    let _ = runtime.add_systems(Update, middle.in_set(Beta).after(Alpha));
    let _ = runtime.add_systems(Update, write_right.in_set(Gamma).after(Beta));
    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let source_descriptor = descriptor(&inspection, "::write_left");
    let target_descriptor = descriptor(&inspection, "::write_right");
    let path = inspection
        .precedence_path(&source_descriptor, &target_descriptor)
        .unwrap();
    assert_eq!(path.systems().len(), 3);
    assert_eq!(path.edges().len(), 2);
    assert!(inspection.access_ambiguities().is_empty());
    assert!(
        inspection
            .pairwise_concurrency(&source_descriptor, &target_descriptor)
            .unwrap()
            .precedence_path()
            .is_some()
    );
}

#[test]
fn access_ambiguity_and_pairwise_assessment_preserve_both_facts() {
    let mut unordered = Runtime::new();
    let _ = unordered.add_systems(Update, write_left);
    let _ = unordered.add_systems(Update, write_right);
    let inspection = unordered.inspect_schedule::<Update>().unwrap().unwrap();
    let left = descriptor(&inspection, "::write_left");
    let right = descriptor(&inspection, "::write_right");
    assert_eq!(inspection.access_ambiguities().len(), 1);
    assert_eq!(
        inspection.access_ambiguities()[0].conflicts()[0].kind(),
        ScheduleAccessConflictKind::WriteWrite
    );
    let assessment = inspection.pairwise_concurrency(&left, &right).unwrap();
    assert!(assessment.is_prevented());
    assert!(!assessment.is_unconstrained());
    assert!(assessment.precedence_path().is_none());

    let mut ordered = Runtime::new();
    let _ = ordered.add_systems(Update, write_left.before(Alpha));
    let _ = ordered.add_systems(Update, write_right.in_set(Alpha));
    let ordered_inspection = ordered.inspect_schedule::<Update>().unwrap().unwrap();
    let left = descriptor(&ordered_inspection, "::write_left");
    let right = descriptor(&ordered_inspection, "::write_right");
    assert!(ordered_inspection.access_ambiguities().is_empty());
    let assessment = ordered_inspection
        .pairwise_concurrency(&left, &right)
        .unwrap();
    assert!(assessment.is_prevented());
    assert!(assessment.precedence_path().is_some());
    assert_eq!(
        assessment.access_conflicts()[0].kind(),
        ScheduleAccessConflictKind::WriteWrite
    );
}

#[test]
fn unconstrained_pairs_are_not_described_as_parallel() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, source);
    let _ = runtime.add_systems(Update, target);
    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let first = descriptor(&inspection, "::source");
    let second = descriptor(&inspection, "::target");
    let assessment = inspection.pairwise_concurrency(&first, &second).unwrap();
    assert!(assessment.is_unconstrained());
    assert!(assessment.precedence_path().is_none());
    assert!(assessment.access_conflicts().is_empty());
}

#[test]
fn publication_projection_reuses_exact_frontier_associations() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        deferred_producer
            .on_invoker_thread()
            .in_set(Alpha)
            .after_if_present(Beta),
    );
    let _ = runtime.add_systems(Update, target.after(Alpha));
    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    assert_eq!(inspection.publication_frontiers().len(), 1);
    let frontier = &inspection.publication_frontiers()[0];
    assert_eq!(frontier.ordinal(), 0);
    assert!(
        frontier.obligations().iter().any(|obligation| matches!(
            obligation,
            SchedulePublicationObligation::Precedence { .. }
        ))
    );
    assert!(frontier.obligations().iter().any(|obligation| matches!(
        obligation,
        SchedulePublicationObligation::Completion { producer } if producer.name().ends_with("::deferred_producer")
    )));
    let producer = descriptor(&inspection, "::deferred_producer");
    let successor = descriptor(&inspection, "::target");
    assert!(
        inspection
            .pairwise_concurrency(&producer, &successor)
            .unwrap()
            .precedence_path()
            .is_some()
    );
}

#[test]
fn a_frontier_does_not_create_unrelated_pairwise_precedence() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, unrelated_deferred.on_invoker_thread().in_set(Alpha));
    let _ = runtime.add_systems(Update, deferred_producer.on_invoker_thread().in_set(Beta));
    let _ = runtime.add_systems(Update, target.after(Beta));
    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let unrelated = descriptor(&inspection, "::unrelated_deferred");
    let target = descriptor(&inspection, "::target");
    let assessment = inspection
        .pairwise_concurrency(&unrelated, &target)
        .unwrap();
    assert!(assessment.is_unconstrained());
    assert!(assessment.precedence_path().is_none());
}

#[test]
fn completion_only_projection_has_one_frontier_without_a_cut() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, deferred_producer.on_invoker_thread());
    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    assert_eq!(inspection.publication_frontiers().len(), 1);
    assert!(matches!(
        inspection.publication_frontiers()[0].obligations(),
        [SchedulePublicationObligation::Completion { .. }]
    ));
}

#[test]
fn inspection_is_observational_and_missing_schedules_are_empty() {
    let mut world = World::new();
    world.insert_resource(Counter::default());
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(Update, increments);
    assert!(runtime.inspect_schedule::<Unused>().unwrap().is_none());
    let _ = runtime.inspect_schedule::<Update>().unwrap();
    assert_eq!(world.resource::<Counter>().unwrap().0, 0);
    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert_eq!(world.resource::<Counter>().unwrap().0, 1);
}

fn cycle_error(reverse: bool) -> ScheduleValidationError {
    let mut runtime = Runtime::new();
    if reverse {
        let _ = runtime.add_systems(Update, cycle_b.in_set(Beta).after(Alpha));
        let _ = runtime.add_systems(Update, cycle_a.in_set(Alpha).after(Beta));
    } else {
        let _ = runtime.add_systems(Update, cycle_a.in_set(Alpha).after(Beta));
        let _ = runtime.add_systems(Update, cycle_b.in_set(Beta).after(Alpha));
    }
    schedule_error(runtime.inspect_schedule::<Update>().unwrap_err())
}

#[test]
fn inspection_reports_registration_mobility_without_changing_ordering() {
    let mut transferable_runtime = Runtime::new();
    let _ = transferable_runtime.add_systems(Update, mobility_target);
    let transferable = transferable_runtime
        .inspect_schedule::<Update>()
        .unwrap()
        .unwrap();
    let transferable_descriptor = descriptor(&transferable, "::mobility_target");
    assert_eq!(
        transferable.execution_mobility(&transferable_descriptor),
        Some(ExecutionMobility::Transferable)
    );

    let mut local_runtime = Runtime::new();
    let _ = local_runtime.add_systems(
        Update,
        mobility_target
            .on_invoker_thread()
            .in_set(Alpha)
            .after(Beta)
            .before_if_present(Gamma),
    );
    let _ = local_runtime.add_systems(Update, target.in_set(Beta));
    let local = local_runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let local_descriptor = descriptor(&local, "::mobility_target");
    assert_eq!(
        local.execution_mobility(&local_descriptor),
        Some(ExecutionMobility::InvokerThreadOnly)
    );
    assert_eq!(local.precedence_edges().len(), 1);
}

#[test]
fn cycles_are_structured_and_declaration_permutations_are_equal() {
    let first = cycle_error(false);
    let second = cycle_error(true);
    assert_eq!(first, second);
    match first {
        ScheduleValidationError::OrderingCycle { schedule, cycle } => {
            assert!(schedule.name().ends_with("::Update"));
            assert_eq!(cycle.reasons().len(), 2);
            assert!(cycle.reasons().iter().all(|reason| {
                reason.predecessor().name().contains("cycle_")
                    && reason.successor().name().contains("cycle_")
            }));
        }
        other => panic!("expected structured cycle, got {other:?}"),
    }
}

#[test]
fn cycle_reason_selection_keeps_one_canonical_reason_per_edge() {
    let mut runtime = Runtime::new();
    let _ = runtime.add_systems(
        Update,
        cycle_a
            .in_set(Alpha)
            .before(Beta)
            .before_if_present(Gamma)
            .after(Beta),
    );
    let _ = runtime.add_systems(Update, cycle_b.in_set(Beta).after(Alpha));
    let _ = runtime.add_systems(Update, cycle_c.in_set(Gamma));
    let error = schedule_error(runtime.inspect_schedule::<Update>().unwrap_err());
    let ScheduleValidationError::OrderingCycle { cycle, .. } = error else {
        panic!("expected structured cycle");
    };
    assert!(!cycle.reasons().is_empty());
}

#[allow(dead_code)]
fn _assert_cycle_type_is_public(_: ScheduleOrderingCycle) {}

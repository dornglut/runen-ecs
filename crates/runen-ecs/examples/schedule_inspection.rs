use runen_ecs::prelude::*;
use runen_ecs::{ExecutionMobility, OrderingPresence, ScheduleOrderingResolutionKind};

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {}

#[derive(Copy, Clone)]
struct Ordered;

impl SystemSet for Ordered {}

#[derive(Copy, Clone)]
struct OptionalTarget;

impl SystemSet for OptionalTarget {}

#[derive(Resource)]
struct SharedCounter(u32);

fn ordered_source() {}

fn ordered_target() {}

fn write_left(mut counter: ResMut<SharedCounter>) {
    counter.0 += 1;
}

fn write_right(mut counter: ResMut<SharedCounter>) {
    counter.0 += 1;
}

fn free_a() {}

fn free_b() {}

fn main() {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            ordered_source
                .before(Ordered)
                .before_if_present(OptionalTarget),
        )
        .unwrap();
    runtime
        .add_systems(Update, ordered_target.in_set(Ordered))
        .unwrap();
    runtime.add_systems(Update, write_left).unwrap();
    runtime.add_systems(Update, write_right).unwrap();
    runtime.add_systems(Update, free_a).unwrap();
    runtime.add_systems(Update, free_b).unwrap();

    let inspection = runtime
        .inspect_schedule::<Update>()
        .expect("schedule should validate")
        .expect("Update should exist");

    // systems() is diagnostic presentation order only. These descriptors are
    // facts/handles for this inspection snapshot, not persistent external IDs.
    let descriptor = |suffix: &str| {
        inspection
            .systems()
            .iter()
            .find(|system| system.name().ends_with(suffix))
            .unwrap_or_else(|| panic!("missing system ending in {suffix}"))
            .clone()
    };

    let source = descriptor("::ordered_source");
    let target = descriptor("::ordered_target");
    let left = descriptor("::write_left");
    let right = descriptor("::write_right");
    let free_a = descriptor("::free_a");
    let free_b = descriptor("::free_b");

    let required = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.target_set().name() == Ordered.name())
        .expect("required ordering should be represented");
    assert_eq!(required.presence(), OrderingPresence::Required);
    assert!(matches!(
        required.kind(),
        ScheduleOrderingResolutionKind::Resolved { .. }
    ));

    let optional = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.target_set().name() == OptionalTarget.name())
        .expect("optional ordering should be represented");
    assert_eq!(optional.presence(), OrderingPresence::Optional);
    assert!(matches!(
        optional.kind(),
        ScheduleOrderingResolutionKind::AbsentOptional
    ));

    let path = inspection
        .precedence_path(&source, &target)
        .expect("required ordering should create a precedence path");
    assert_eq!(path.systems(), &[source.clone(), target.clone()]);

    // The writers have an access conflict, but no semantic precedence edge.
    let ambiguity = inspection
        .access_ambiguities()
        .iter()
        .find(|ambiguity| {
            (ambiguity.first() == &left && ambiguity.second() == &right)
                || (ambiguity.first() == &right && ambiguity.second() == &left)
        })
        .expect("unordered writers should be diagnosed");
    assert!(!ambiguity.conflicts().is_empty());

    let writer_assessment = inspection
        .pairwise_concurrency(&left, &right)
        .expect("distinct systems should have an assessment");
    assert!(writer_assessment.is_prevented());
    assert!(writer_assessment.precedence_path().is_none());
    assert!(!writer_assessment.access_conflicts().is_empty());

    // "Unconstrained" is a semantic diagnostic fact, not a guarantee that the
    // current executor will run these systems in parallel.
    let free_assessment = inspection
        .pairwise_concurrency(&free_a, &free_b)
        .expect("distinct systems should have an assessment");
    assert!(free_assessment.is_unconstrained());

    // Mobility reports registration eligibility, not current worker placement.
    assert_eq!(
        inspection.execution_mobility(&source),
        Some(ExecutionMobility::Transferable)
    );

    println!(
        "systems: {}, precedence edges: {}, access ambiguities: {}",
        inspection.systems().len(),
        inspection.precedence_edges().len(),
        inspection.access_ambiguities().len()
    );
}

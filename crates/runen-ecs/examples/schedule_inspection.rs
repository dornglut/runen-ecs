use runen_ecs::prelude::*;
use runen_ecs::{ExecutionMobility, OrderingPresence, ScheduleOrderingResolutionKind};

#[derive(ScheduleLabel)]
struct Update;

#[derive(Copy, Clone)]
struct Simulation;

impl SystemSet for Simulation {}

#[derive(Copy, Clone)]
struct OptionalTelemetry;

impl SystemSet for OptionalTelemetry {}

#[derive(Resource)]
struct SharedMetrics(u32);

fn prepare_frame() {}

fn simulate() {}

fn accumulate_metrics(mut metrics: ResMut<SharedMetrics>) {
    metrics.0 += 1;
}

fn reset_metrics(mut metrics: ResMut<SharedMetrics>) {
    metrics.0 = 0;
}

fn main() {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            prepare_frame
                .before(Simulation)
                .before_if_present(OptionalTelemetry),
        )
        .unwrap();
    runtime
        .add_systems(Update, simulate.in_set(Simulation))
        .unwrap();
    runtime.add_systems(Update, accumulate_metrics).unwrap();
    runtime.add_systems(Update, reset_metrics).unwrap();

    let inspection = runtime
        .inspect_schedule::<Update>()
        .expect("schedule should validate")
        .expect("Update should exist");

    // systems() is diagnostic presentation order only, not execution order.
    // Descriptors are facts/handles for this snapshot, not persistent IDs.
    assert_eq!(inspection.systems().len(), 4);

    let required = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.target_set().name() == Simulation.name())
        .expect("required ordering should be represented");
    assert_eq!(required.presence(), OrderingPresence::Required);
    let targets = required
        .kind()
        .target_systems()
        .expect("required ordering should resolve to a system");
    assert_eq!(targets.len(), 1);

    let source = required.source();
    let target = &targets[0];
    let path = inspection
        .precedence_path(source, target)
        .expect("required ordering should create a precedence path");
    assert_eq!(path.systems(), &[source.clone(), target.clone()]);

    let optional = inspection
        .ordering_resolutions()
        .iter()
        .find(|resolution| resolution.target_set().name() == OptionalTelemetry.name())
        .expect("optional ordering should be represented");
    assert_eq!(optional.presence(), OrderingPresence::Optional);
    assert!(matches!(
        optional.kind(),
        ScheduleOrderingResolutionKind::AbsentOptional
    ));

    // The two metric systems conflict on a mutable resource, but that access
    // conflict does not invent semantic precedence between them.
    let ambiguity = inspection
        .access_ambiguities()
        .first()
        .expect("unordered metric writers should be diagnosed");
    let writer_assessment = inspection
        .pairwise_concurrency(ambiguity.first(), ambiguity.second())
        .expect("distinct systems should have an assessment");
    assert!(writer_assessment.is_prevented());
    assert!(writer_assessment.precedence_path().is_none());
    assert!(!writer_assessment.access_conflicts().is_empty());

    // "Unconstrained" is also only a diagnostic fact. It does not promise that
    // the current executor will run this pair in parallel.
    let free_assessment = inspection
        .pairwise_concurrency(source, ambiguity.first())
        .expect("distinct systems should have an assessment");
    assert!(free_assessment.is_unconstrained());

    // Mobility reports registration eligibility, not current worker placement.
    assert_eq!(
        inspection.execution_mobility(source),
        Some(ExecutionMobility::Transferable)
    );

    println!(
        "systems: {}, precedence edges: {}, access ambiguities: {}",
        inspection.systems().len(),
        inspection.precedence_edges().len(),
        inspection.access_ambiguities().len()
    );
}

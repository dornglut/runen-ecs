use runen_ecs::prelude::*;
use runen_ecs::{
    EntityError, ExecutionMobility, RuntimeError, ScheduleAccessConflictKind, ScheduleAccessDomain,
    SystemParam, TransferableSystemParam, WorldMut,
};
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {}

#[derive(Component)]
struct Marker;

#[derive(Resource, Copy, Clone)]
struct Endpoints {
    source: Entity,
    target: Entity,
}

#[derive(Resource, Default)]
struct Seen {
    directed: usize,
    symmetric: usize,
}

struct Owns;
impl Relation for Owns {
    type Kind = Directed;
}

struct Follows;
impl Relation for Follows {
    type Kind = Directed;
}

struct AlliedWith;
impl Relation for AlliedWith {
    type Kind = Symmetric;
}

#[allow(dead_code)]
struct ThreadBoundRelation(PhantomData<Rc<()>>);
impl Relation for ThreadBoundRelation {
    type Kind = Directed;
}

#[derive(SystemParam)]
struct RelationGroup<'w> {
    owns: Relations<'w, Owns>,
    allies: RelationsMut<'w, AlliedWith>,
}

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

fn shared_shared(_first: Relations<Owns>, _second: Relations<Owns>) {}

fn shared_mutable(_read: Relations<Owns>, _write: RelationsMut<Owns>) {}

fn mutable_mutable(_first: RelationsMut<Owns>, _second: RelationsMut<Owns>) {}

fn distinct_mutable(_owns: RelationsMut<Owns>, _follows: RelationsMut<Follows>) {}

fn relation_and_query(_owns: Relations<Owns>, _query: Query<&Marker>) {}

fn relation_and_world(_owns: Relations<Owns>, _world: WorldMut) {}

fn relation_and_commands(_owns: RelationsMut<Owns>, _commands: Commands) {}

#[test]
fn relation_params_use_relation_scoped_borrow_validation() {
    let mut runtime = Runtime::new();
    runtime.add_systems(Update, shared_shared).unwrap();

    let mut runtime = Runtime::new();
    let error = match runtime.add_systems(Update, shared_mutable) {
        Ok(_) => panic!("shared plus mutable relation borrow must fail"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(message.contains("conflicting param borrows"), "{message}");
    assert!(message.contains("relation"), "{message}");
    assert!(message.contains(Owns::name()), "{message}");

    let mut runtime = Runtime::new();
    let error = match runtime.add_systems(Update, mutable_mutable) {
        Ok(_) => panic!("two mutable relation borrows must fail"),
        Err(error) => error,
    };
    let message = format!("{error:#}");
    assert!(message.contains("conflicting param borrows"), "{message}");
    assert!(message.contains("relation"), "{message}");

    let mut runtime = Runtime::new();
    runtime.add_systems(Update, distinct_mutable).unwrap();

    let mut runtime = Runtime::new();
    runtime.add_systems(Update, relation_and_query).unwrap();

    let mut runtime = Runtime::new();
    runtime.add_systems(Update, relation_and_commands).unwrap();

    let mut runtime = Runtime::new();
    let error = match runtime.add_systems(Update, relation_and_world.on_invoker_thread()) {
        Ok(_) => panic!("WorldMut must conflict with immediate relation access"),
        Err(error) => error,
    };
    assert!(format!("{error:#}").contains("world"));
}

#[test]
fn relation_params_report_relation_access_metadata_and_are_transferable_without_marker_bounds() {
    let read = <Relations<'static, Owns> as SystemParam>::access(&());
    assert_eq!(read.relation_reads().len(), 1);
    assert_eq!(read.relation_reads()[0].name(), Owns::name());
    assert!(read.relation_writes().is_empty());

    let write = <RelationsMut<'static, Owns> as SystemParam>::access(&());
    assert_eq!(write.relation_writes().len(), 1);
    assert_eq!(write.relation_writes()[0].name(), Owns::name());
    assert!(write.relation_reads().is_empty());

    fn assert_transferable<P: TransferableSystemParam>()
    where
        P::State: Send,
    {
    }
    assert_transferable::<Relations<'static, ThreadBoundRelation>>();
    assert_transferable::<RelationsMut<'static, ThreadBoundRelation>>();
}

#[test]
fn derived_and_invoker_thread_relation_params_preserve_the_same_api() {
    fn grouped(mut group: RelationGroup<'_>, endpoints: Res<Endpoints>) {
        assert!(group.owns.contains(endpoints.source, endpoints.target));
        assert!(
            group
                .allies
                .insert(endpoints.source, endpoints.target)
                .unwrap()
        );
    }

    fn local_view(relations: Relations<Owns>, endpoints: Res<Endpoints>) {
        assert!(relations.contains(endpoints.source, endpoints.target));
    }

    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let target = world.spawn(Marker).unwrap();
    world.insert_resource(Endpoints { source, target });
    world
        .relations_mut::<Owns>()
        .insert(source, target)
        .unwrap();

    let mut runtime = Runtime::new();
    runtime.add_systems(Update, grouped).unwrap();
    runtime
        .add_systems(Update, local_view.on_invoker_thread())
        .unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert!(world.relations::<AlliedWith>().contains(source, target));
}

fn deterministic_relation_step(
    mut owns: RelationsMut<Owns>,
    mut allies: RelationsMut<AlliedWith>,
    endpoints: Res<Endpoints>,
    mut seen: ResMut<Seen>,
) {
    assert!(owns.insert(endpoints.source, endpoints.target).unwrap());
    assert!(allies.insert(endpoints.source, endpoints.target).unwrap());

    assert_eq!(
        owns.targets(endpoints.source)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![endpoints.target]
    );
    assert_eq!(
        owns.sources(endpoints.target)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![endpoints.source]
    );
    assert_eq!(
        allies
            .neighbors(endpoints.source)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![endpoints.target]
    );

    seen.directed = owns.len();
    seen.symmetric = allies.len();
}

fn run_relation_fixture(parallel: bool) -> (usize, usize, bool, bool) {
    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let target = world.spawn(Marker).unwrap();
    world.insert_resource(Endpoints { source, target });
    world.insert_resource(Seen::default());

    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, deterministic_relation_step)
        .unwrap();
    if parallel {
        runtime
            .run_schedule_parallel::<Update>(&mut world, 2)
            .unwrap();
    } else {
        runtime.run_schedule::<Update>(&mut world).unwrap();
    }

    let seen = world.resource::<Seen>().unwrap();
    (
        seen.directed,
        seen.symmetric,
        world.relations::<Owns>().contains(source, target),
        world.relations::<AlliedWith>().contains(target, source),
    )
}

struct SelfAllowed;
impl Relation for SelfAllowed {
    type Kind = Directed;
    const SELF: SelfRelation = SelfRelation::Allow;
}

#[test]
fn mutable_relation_param_preserves_remove_clear_and_self_policy() {
    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();
    world.insert_resource(Endpoints {
        source,
        target: first,
    });
    world.relations_mut::<Owns>().insert(source, first).unwrap();
    world.relations_mut::<Owns>().insert(source, second).unwrap();

    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            move |mut owns: RelationsMut<Owns>, mut self_allowed: RelationsMut<SelfAllowed>| {
                assert!(owns.remove(source, first).unwrap());
                assert_eq!(owns.clear_entity(source).unwrap(), 1);
                assert!(self_allowed.insert(source, source).unwrap());
            },
        )
        .unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();

    assert!(world.relations::<Owns>().is_empty());
    assert!(world
        .relations::<SelfAllowed>()
        .contains(source, source));

    let mut forbidden = Runtime::new();
    forbidden
        .add_systems(Update, move |mut owns: RelationsMut<Owns>| {
            assert!(matches!(
                owns.insert(source, source),
                Err(runen_ecs::RelationError::SelfReference { entity, .. }) if entity == source
            ));
        })
        .unwrap();
    forbidden.run_schedule::<Update>(&mut world).unwrap();
}

#[test]
fn serial_and_parallel_relation_params_preserve_the_same_facts() {
    assert_eq!(run_relation_fixture(false), (1, 1, true, true));
    assert_eq!(run_relation_fixture(true), (1, 1, true, true));
}

#[test]
fn parallel_relation_validation_matches_direct_entity_error_classes_and_precedence() {
    let mut world = World::new();

    let stale = world.spawn(Marker).unwrap();
    world.despawn(stale).unwrap();
    let valid = world.spawn(Marker).unwrap();
    assert_eq!(stale.index(), valid.index());

    let freed = world.spawn(Marker).unwrap();
    world.despawn(freed).unwrap();

    let mut foreign_world = World::new();
    let foreign = foreign_world.spawn(Marker).unwrap();

    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, move |mut owns: RelationsMut<Owns>| {
            assert!(matches!(
                owns.insert(foreign, valid),
                Err(runen_ecs::RelationError::Entity(EntityError::ForeignWorld { entity }))
                    if entity == foreign
            ));
            assert!(matches!(
                owns.insert(stale, valid),
                Err(runen_ecs::RelationError::Entity(EntityError::StaleGeneration {
                    entity,
                    ..
                })) if entity == stale
            ));
            assert!(matches!(
                owns.insert(freed, foreign),
                Err(runen_ecs::RelationError::Entity(EntityError::AlreadyFreed { entity }))
                    if entity == freed
            ));
            assert!(owns.is_empty());
        })
        .unwrap();

    runtime
        .run_schedule_parallel::<Update>(&mut world, 1)
        .unwrap();
    assert!(world.relations::<Owns>().is_empty());
}

#[derive(Debug)]
struct Failure;

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("intentional relation system failure")
    }
}

impl std::error::Error for Failure {}

#[test]
fn parallel_relation_mutation_is_not_rolled_back_after_system_error() {
    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let target = world.spawn(Marker).unwrap();
    world.insert_resource(Endpoints { source, target });

    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            |mut owns: RelationsMut<Owns>, endpoints: Res<Endpoints>| {
                assert!(owns.insert(endpoints.source, endpoints.target).unwrap());
                Err::<(), _>(Failure)
            },
        )
        .unwrap();

    let result = runtime.run_schedule_parallel::<Update>(&mut world, 1);
    assert!(matches!(result, Err(RuntimeError::System { .. })));
    assert!(world.relations::<Owns>().contains(source, target));
}

fn inspect_read(_owns: Relations<Owns>) {}
fn inspect_write(_owns: RelationsMut<Owns>) {}
fn inspect_other_write(_follows: RelationsMut<Follows>) {}

#[test]
fn schedule_inspection_reports_relation_conflicts_without_inventing_precedence() {
    let mut runtime = Runtime::new();
    runtime.add_systems(Update, inspect_read).unwrap();
    runtime.add_systems(Update, inspect_write).unwrap();
    runtime.add_systems(Update, inspect_other_write).unwrap();

    let inspection = runtime.inspect_schedule::<Update>().unwrap().unwrap();
    let read = descriptor(&inspection, "::inspect_read");
    let write = descriptor(&inspection, "::inspect_write");
    let other = descriptor(&inspection, "::inspect_other_write");

    let assessment = inspection.pairwise_concurrency(&read, &write).unwrap();
    assert!(assessment.precedence_path().is_none());
    assert_eq!(assessment.access_conflicts().len(), 1);
    assert_eq!(
        assessment.access_conflicts()[0].domain(),
        ScheduleAccessDomain::Relation
    );
    assert_eq!(
        assessment.access_conflicts()[0].kind(),
        ScheduleAccessConflictKind::ReadWrite
    );
    assert_eq!(assessment.access_conflicts()[0].target(), Owns::name());

    let unrelated = inspection.pairwise_concurrency(&write, &other).unwrap();
    assert!(unrelated.is_unconstrained());
    assert!(unrelated.access_conflicts().is_empty());

    assert!(inspection.access_ambiguities().iter().any(|ambiguity| {
        ambiguity
            .conflicts()
            .iter()
            .any(|conflict| conflict.domain() == ScheduleAccessDomain::Relation)
    }));
    assert_eq!(
        inspection.execution_mobility(&read),
        Some(ExecutionMobility::Transferable)
    );
}

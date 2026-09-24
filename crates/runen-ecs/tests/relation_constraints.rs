use runen_ecs::prelude::*;
use runen_ecs::{CommandError, EntityError, LocalBatchCommands, LocalCommands, RuntimeError};
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Component)]
struct Marker;

struct Owns;
impl Relation for Owns {
    type Kind = Directed;
}

struct ChildOf;
impl Relation for ChildOf {
    type Kind = Directed;
    const CONSTRAINTS: RelationConstraints = RelationConstraints::new()
        .source_cardinality(SourceCardinality::One)
        .cycles(CyclePolicy::Forbid);
}

struct AlternateParent;
impl Relation for AlternateParent {
    type Kind = Directed;
    const CONSTRAINTS: RelationConstraints = RelationConstraints::new()
        .source_cardinality(SourceCardinality::One)
        .cycles(CyclePolicy::Forbid);
}

struct SingleTarget;
impl Relation for SingleTarget {
    type Kind = Directed;
    const CONSTRAINTS: RelationConstraints =
        RelationConstraints::new().source_cardinality(SourceCardinality::One);
}

struct AcyclicMany;
impl Relation for AcyclicMany {
    type Kind = Directed;
    const CONSTRAINTS: RelationConstraints = RelationConstraints::new().cycles(CyclePolicy::Forbid);
}

struct UnsupportedSymmetricOne;
impl Relation for UnsupportedSymmetricOne {
    type Kind = Symmetric;
    const CONSTRAINTS: RelationConstraints =
        RelationConstraints::new().source_cardinality(SourceCardinality::One);
}

struct UnsupportedSymmetricCycle;
impl Relation for UnsupportedSymmetricCycle {
    type Kind = Symmetric;
    const CONSTRAINTS: RelationConstraints = RelationConstraints::new().cycles(CyclePolicy::Forbid);
}

#[derive(Copy, Clone)]
struct Update;
impl ScheduleLabel for Update {}

#[test]
fn constraint_value_is_const_composable_and_defaults_to_unconstrained() {
    assert_eq!(
        RelationConstraints::UNCONSTRAINED.source_cardinality_value(),
        SourceCardinality::Many
    );
    assert_eq!(
        RelationConstraints::UNCONSTRAINED.cycle_policy(),
        CyclePolicy::Allow
    );
    assert_eq!(Owns::CONSTRAINTS, RelationConstraints::UNCONSTRAINED);
    assert_eq!(
        ChildOf::CONSTRAINTS.source_cardinality_value(),
        SourceCardinality::One
    );
    assert_eq!(ChildOf::CONSTRAINTS.cycle_policy(), CyclePolicy::Forbid);
}

#[test]
fn unconstrained_relations_keep_existing_many_to_many_behavior() {
    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();

    assert!(world.relations_mut::<Owns>().insert(source, first).unwrap());
    assert!(
        world
            .relations_mut::<Owns>()
            .insert(source, second)
            .unwrap()
    );
    assert_eq!(
        world
            .relations::<Owns>()
            .targets(source)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![first, second]
    );
}

#[test]
fn source_one_insertion_replaces_atomically_and_preserves_boolean_semantics() {
    let mut world = World::new();
    let child = world.spawn(Marker).unwrap();
    let other_child = world.spawn(Marker).unwrap();
    let first_parent = world.spawn(Marker).unwrap();
    let second_parent = world.spawn(Marker).unwrap();

    let mut parents = world.relations_mut::<SingleTarget>();
    assert!(parents.insert(child, first_parent).unwrap());
    assert!(!parents.insert(child, first_parent).unwrap());
    assert!(parents.insert(child, second_parent).unwrap());
    assert!(parents.insert(other_child, second_parent).unwrap());

    assert!(!parents.contains(child, first_parent));
    assert!(parents.contains(child, second_parent));
    assert_eq!(
        parents.targets(child).unwrap().iter().collect::<Vec<_>>(),
        vec![second_parent]
    );
    assert_eq!(
        parents
            .sources(first_parent)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        Vec::<Entity>::new()
    );
    assert_eq!(
        parents
            .sources(second_parent)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![child, other_child]
    );
    assert_eq!(
        parents.iter().collect::<Vec<_>>(),
        vec![(child, second_parent), (other_child, second_parent)]
    );
}

#[test]
fn source_one_remove_and_clear_keep_existing_incident_edge_meaning() {
    let mut world = World::new();
    let child = world.spawn(Marker).unwrap();
    let other = world.spawn(Marker).unwrap();
    let parent = world.spawn(Marker).unwrap();

    let mut parents = world.relations_mut::<ChildOf>();
    assert!(parents.insert(child, parent).unwrap());
    assert!(!parents.remove(child, other).unwrap());
    assert!(parents.remove(child, parent).unwrap());
    assert!(parents.insert(child, other).unwrap());
    assert!(parents.insert(parent, other).unwrap());
    assert_eq!(parents.clear_entity(other).unwrap(), 2);
    assert!(parents.is_empty());
}

#[test]
fn self_reference_still_precedes_cycle_policy() {
    let mut world = World::new();
    let entity = world.spawn(Marker).unwrap();

    assert!(matches!(
        world.relations_mut::<ChildOf>().insert(entity, entity),
        Err(RelationError::SelfReference { entity: found, .. }) if found == entity
    ));
}

#[test]
fn acyclic_relations_reject_two_node_and_deeper_cycles() {
    let mut world = World::new();
    let a = world.spawn(Marker).unwrap();
    let b = world.spawn(Marker).unwrap();
    let c = world.spawn(Marker).unwrap();
    let d = world.spawn(Marker).unwrap();

    {
        let mut relation = world.relations_mut::<AcyclicMany>();
        relation.insert(a, b).unwrap();
        assert!(matches!(
            relation.insert(b, a),
            Err(RelationError::Cycle { source_entity, target_entity, .. }) if source_entity == b && target_entity == a
        ));
        relation.insert(b, c).unwrap();
        relation.insert(c, d).unwrap();
        assert!(matches!(
            relation.insert(d, a),
            Err(RelationError::Cycle { source_entity, target_entity, .. }) if source_entity == d && target_entity == a
        ));
    }

    assert_eq!(
        world.relations::<AcyclicMany>().iter().collect::<Vec<_>>(),
        vec![(a, b), (b, c), (c, d)]
    );
}

#[test]
fn rejected_source_one_reparent_preserves_previous_parent() {
    let mut world = World::new();
    let root = world.spawn(Marker).unwrap();
    let parent = world.spawn(Marker).unwrap();
    let child = world.spawn(Marker).unwrap();
    let grandchild = world.spawn(Marker).unwrap();

    {
        let mut hierarchy = world.relations_mut::<ChildOf>();
        hierarchy.insert(parent, root).unwrap();
        hierarchy.insert(child, parent).unwrap();
        hierarchy.insert(grandchild, child).unwrap();

        assert!(matches!(
            hierarchy.insert(child, grandchild),
            Err(RelationError::Cycle { source_entity, target_entity, .. })
                if source_entity == child && target_entity == grandchild
        ));
        assert!(hierarchy.contains(child, parent));
        assert!(!hierarchy.contains(child, grandchild));

        assert!(hierarchy.insert(child, root).unwrap());
        assert!(!hierarchy.contains(child, parent));
        assert!(hierarchy.contains(child, root));
    }
}

#[test]
fn invalid_endpoints_precede_constraint_checks_and_leave_state_unchanged() {
    let mut world = World::new();
    let source = world.spawn(Marker).unwrap();
    let parent = world.spawn(Marker).unwrap();
    world
        .relations_mut::<ChildOf>()
        .insert(source, parent)
        .unwrap();

    let freed = world.spawn(Marker).unwrap();
    world.despawn(freed).unwrap();
    let mut foreign_world = World::new();
    let foreign = foreign_world.spawn(Marker).unwrap();

    assert!(matches!(
        world.relations_mut::<ChildOf>().insert(freed, source),
        Err(RelationError::Entity(EntityError::AlreadyFreed { entity })) if entity == freed
    ));
    assert!(matches!(
        world.relations_mut::<ChildOf>().insert(source, foreign),
        Err(RelationError::Entity(EntityError::ForeignWorld { entity })) if entity == foreign
    ));
    assert!(world.relations::<ChildOf>().contains(source, parent));
}

#[test]
fn unsupported_symmetric_constraints_are_configuration_failures() {
    let mut world = World::new();
    let first = world.spawn(Marker).unwrap();
    let second = world.spawn(Marker).unwrap();

    let one = catch_unwind(AssertUnwindSafe(|| {
        let _ = world
            .relations_mut::<UnsupportedSymmetricOne>()
            .insert(first, second);
    }));
    assert!(one.is_err());

    let cycle = catch_unwind(AssertUnwindSafe(|| {
        let _ = world
            .relations_mut::<UnsupportedSymmetricCycle>()
            .insert(first, second);
    }));
    assert!(cycle.is_err());
}

#[test]
fn serial_and_parallel_system_params_share_constraint_semantics() {
    fn run(parallel: bool) -> Vec<(u32, u32)> {
        let mut world = World::new();
        let root = world.spawn(Marker).unwrap();
        let first_parent = world.spawn(Marker).unwrap();
        let child = world.spawn(Marker).unwrap();

        world
            .relations_mut::<ChildOf>()
            .insert(child, first_parent)
            .unwrap();

        let mut runtime = Runtime::new();
        runtime
            .add_systems(Update, move |mut hierarchy: RelationsMut<ChildOf>| {
                assert!(hierarchy.insert(child, root).unwrap());
                assert!(matches!(
                    hierarchy.insert(root, child),
                    Err(RelationError::Cycle { .. })
                ));
                assert!(hierarchy.contains(child, root));
            })
            .unwrap();

        if parallel {
            runtime
                .run_schedule_parallel::<Update>(&mut world, 1)
                .unwrap();
        } else {
            runtime.run_schedule::<Update>(&mut world).unwrap();
        }

        world
            .relations::<ChildOf>()
            .iter()
            .map(|(source, target)| (source.index(), target.index()))
            .collect()
    }

    assert_eq!(run(false), run(true));
}

#[test]
fn rejected_worker_reparent_keeps_previous_edge() {
    let mut world = World::new();
    let root = world.spawn(Marker).unwrap();
    let child = world.spawn(Marker).unwrap();
    let grandchild = world.spawn(Marker).unwrap();

    {
        let mut hierarchy = world.relations_mut::<ChildOf>();
        hierarchy.insert(child, root).unwrap();
        hierarchy.insert(grandchild, child).unwrap();
    }

    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, move |mut hierarchy: RelationsMut<ChildOf>| {
            assert!(matches!(
                hierarchy.insert(child, grandchild),
                Err(RelationError::Cycle { .. })
            ));
            assert!(hierarchy.contains(child, root));
        })
        .unwrap();
    runtime
        .run_schedule_parallel::<Update>(&mut world, 1)
        .unwrap();

    assert!(world.relations::<ChildOf>().contains(child, root));
    assert!(!world.relations::<ChildOf>().contains(child, grandchild));
}

#[test]
fn distinct_constrained_relation_types_remain_access_independent() {
    let mut runtime = Runtime::new();
    runtime
        .add_systems(
            Update,
            (
                |_first: RelationsMut<ChildOf>| {},
                |_second: RelationsMut<AlternateParent>| {},
            ),
        )
        .unwrap();

    let mut conflicting = Runtime::new();
    let result = conflicting.add_systems(
        Update,
        |_first: RelationsMut<ChildOf>, _second: RelationsMut<ChildOf>| {},
    );
    assert!(matches!(result, Err(RuntimeError::Setup { .. })));
}

#[test]
fn commands_inherit_replacement_and_cycle_rejection() {
    let mut world = World::new();
    let root = world.spawn(Marker).unwrap();
    let first_parent = world.spawn(Marker).unwrap();
    let child = world.spawn(Marker).unwrap();

    world
        .relations_mut::<ChildOf>()
        .insert(child, first_parent)
        .unwrap();

    let mut commands = Commands::new();
    commands.insert_relation::<ChildOf>(child, root);
    commands.apply(&mut world).unwrap();
    assert!(world.relations::<ChildOf>().contains(child, root));

    let mut failing = Commands::new();
    failing.insert_relation::<ChildOf>(root, child);
    assert!(matches!(
        failing.apply(&mut world),
        Err(CommandError::Relation(RelationError::Cycle { source_entity, target_entity, .. }))
            if source_entity == root && target_entity == child
    ));
    assert!(world.relations::<ChildOf>().contains(child, root));
    assert!(!world.relations::<ChildOf>().contains(root, child));
}

#[test]
fn batch_commands_stop_after_cycle_while_preserving_earlier_success() {
    let mut world = World::new();
    let root = world.spawn(Marker).unwrap();
    let child = world.spawn(Marker).unwrap();
    let earlier = world.spawn(Marker).unwrap();
    let later = world.spawn(Marker).unwrap();

    world
        .relations_mut::<ChildOf>()
        .insert(child, root)
        .unwrap();

    let mut commands = Commands::new();
    commands.batch(|batch| {
        batch.insert_relation::<ChildOf>(earlier, root);
        batch.insert_relation::<ChildOf>(root, child);
        batch.insert_relation::<ChildOf>(later, root);
    });

    assert!(matches!(
        commands.apply(&mut world),
        Err(CommandError::Relation(RelationError::Cycle { .. }))
    ));
    assert!(world.relations::<ChildOf>().contains(earlier, root));
    assert!(!world.relations::<ChildOf>().contains(later, root));
    assert!(world.relations::<ChildOf>().contains(child, root));
}

#[test]
fn local_command_paths_share_constrained_relation_semantics() {
    let mut world = World::new();
    let root = world.spawn(Marker).unwrap();
    let first_parent = world.spawn(Marker).unwrap();
    let child = world.spawn(Marker).unwrap();
    let later = world.spawn(Marker).unwrap();

    world
        .relations_mut::<ChildOf>()
        .insert(child, first_parent)
        .unwrap();

    let mut local = LocalCommands::new();
    local.insert_relation::<ChildOf>(child, root);
    local.apply(&mut world).unwrap();
    assert!(world.relations::<ChildOf>().contains(child, root));

    let mut local_batch = LocalBatchCommands::new();
    local_batch.insert_relation::<ChildOf>(later, root);
    local_batch.insert_relation::<ChildOf>(root, child);
    assert!(matches!(
        local_batch.apply(&mut world),
        Err(CommandError::Relation(RelationError::Cycle { .. }))
    ));
    assert!(world.relations::<ChildOf>().contains(later, root));
    assert!(!world.relations::<ChildOf>().contains(root, child));
}

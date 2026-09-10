use runen_ecs::EntityError;
use runen_ecs::prelude::*;
use runen_ecs::{QueryTypeAccess, SystemParam};
use std::any::TypeId;

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Player;

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Disabled;

#[derive(Debug, Copy, Clone, PartialEq, runen_ecs::Component, runen_ecs::Resource)]
struct Health(i32);

#[derive(Debug, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Name(String);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct Frame(u64);

#[derive(Debug, PartialEq, runen_ecs::Bundle)]
struct CombatBundle {
    health: Health,
    name: Name,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct B(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct C(i32);

#[derive(Copy, Clone)]
struct WorldUpdate;

impl ScheduleLabel for WorldUpdate {
    fn name() -> &'static str {
        "WorldUpdate"
    }
}

#[derive(Copy, Clone)]
struct SpawnStage;

impl SystemSet for SpawnStage {
    fn name() -> &'static str {
        "SpawnStage"
    }
}

#[derive(Copy, Clone)]
struct ObserveStage;

impl SystemSet for ObserveStage {
    fn name() -> &'static str {
        "ObserveStage"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct SpawnGate(bool);

#[derive(Debug, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct AddedHealthCounts(Vec<usize>);

#[test]
fn spawn_query_and_entity_access_work() {
    let mut world = World::new();
    let entity = world
        .spawn((
            Player,
            Position { x: 1.0, y: 2.0 },
            Velocity { x: 0.5, y: -1.0 },
        ))
        .expect("spawn should succeed");

    let position = world.require::<Position>(entity).unwrap();
    assert_eq!(position.x, 1.0);
    assert_eq!(position.y, 2.0);

    let entity_ref = world.entity(entity).unwrap();
    assert!(entity_ref.contains::<Player>());
    assert!(entity_ref.contains::<Velocity>());

    let query = world
        .query_state::<(Entity, &Position), ()>()
        .with::<Player>();
    let seen: Vec<_> = query
        .iter(&world)
        .map(|(entity, position)| (entity, position.x, position.y))
        .collect();
    assert_eq!(seen, vec![(entity, 1.0, 2.0)]);
}

#[test]
fn query_filters_support_unified_iter_for_mutation() {
    let mut world = World::new();
    let active = world
        .spawn((Position { x: 0.0, y: 0.0 }, Velocity { x: 2.0, y: 1.0 }))
        .expect("spawn should succeed");
    let disabled = world
        .spawn((
            Position { x: 5.0, y: 5.0 },
            Velocity { x: 9.0, y: 9.0 },
            Disabled,
        ))
        .expect("spawn should succeed");

    let query = world
        .query_state::<(&mut Position, &Velocity), ()>()
        .without::<Disabled>();
    for (position, velocity) in query.iter(&mut world) {
        position.x += velocity.x;
        position.y += velocity.y;
    }

    assert_eq!(
        world.require::<Position>(active).unwrap(),
        &Position { x: 2.0, y: 1.0 }
    );
    assert_eq!(
        world.require::<Position>(disabled).unwrap(),
        &Position { x: 5.0, y: 5.0 }
    );
}

#[test]
fn entity_mut_bundle_insert_and_remove_work() {
    let mut world = World::new();
    let entity = world.spawn(Player).expect("spawn should succeed");

    {
        let mut entity_mut = world.entity_mut(entity).unwrap();
        entity_mut
            .insert(CombatBundle {
                health: Health(10),
                name: Name("hero".to_string()),
            })
            .unwrap();
        entity_mut.require_mut::<Health>().unwrap().0 -= 3;
    }

    assert_eq!(world.require::<Health>(entity).unwrap(), &Health(7));

    let removed: CombatBundle = world.remove(entity).unwrap();
    assert_eq!(removed.health, Health(7));
    assert_eq!(removed.name, Name("hero".to_string()));
    assert!(world.get::<Health>(entity).is_none());
}

#[test]
fn resources_and_change_ticks_work() {
    let mut world = World::new();
    let start = world.current_change_tick();
    assert!(world.resource_mut::<Velocity>().is_err());
    assert_eq!(world.current_change_tick(), start);
    assert!(!world.resource_changed_since::<Velocity>(start));

    world.insert_resource(Frame(1));
    assert!(world.resource_changed_since::<Frame>(start));

    {
        let frame = world.resource_mut::<Frame>().unwrap();
        frame.0 += 1;
    }

    assert_eq!(world.resource::<Frame>().unwrap().0, 2);
}

#[test]
fn resource_lifecycle_and_change_ticks_work() {
    let mut world = World::new();
    assert!(!world.has_resource::<Frame>());

    let start = world.current_change_tick();
    world.insert_resource(Frame(10));
    {
        let frame = world.resource_mut::<Frame>().unwrap();
        frame.0 += 5;
    }
    let removed = world.remove_resource::<Frame>();

    assert_eq!(removed, Some(Frame(15)));
    assert!(!world.has_resource::<Frame>());

    assert!(world.current_change_tick() > start);
    assert!(world.resource_changed_since::<Frame>(start));
}

#[test]
fn commands_apply_spawn_insert_and_despawn() {
    let mut world = World::new();
    let existing = world
        .spawn(Position { x: 1.0, y: 1.0 })
        .expect("spawn should succeed");
    let doomed = world
        .spawn(Position { x: 99.0, y: 99.0 })
        .expect("spawn should succeed");

    let mut commands = world.commands();
    commands.spawn((Position { x: 3.0, y: 4.0 }, Velocity { x: 0.0, y: 1.0 }));
    commands.insert(existing, Velocity { x: 5.0, y: 6.0 });
    commands.despawn(doomed);
    commands.apply(&mut world).unwrap();

    assert!(world.contains(existing));
    assert!(world.require::<Velocity>(existing).is_ok());
    assert!(!world.contains(doomed));

    let query = world.query_state::<&Position, ()>();
    let positions: Vec<_> = query.iter(&world).copied().collect();
    assert_eq!(positions.len(), 2);
    assert!(positions.contains(&Position { x: 1.0, y: 1.0 }));
    assert!(positions.contains(&Position { x: 3.0, y: 4.0 }));
}

#[test]
fn secondary_indexes_track_updates() {
    let mut world = World::new();
    world.ensure_component_index::<Name, String>(|name| name.0.clone());

    let entity = world
        .spawn(Name("hero".to_string()))
        .expect("spawn should succeed");
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"hero".to_string()),
        Some(entity)
    );

    world.require_mut::<Name>(entity).unwrap().0 = "villain".to_string();
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"hero".to_string()),
        None
    );
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"villain".to_string()),
        Some(entity)
    );
}

#[test]
fn secondary_index_reads_support_shared_world_reference() {
    let mut world = World::new();
    world.ensure_component_index::<Name, String>(|name| name.0.clone());
    world.ensure_component_index_named::<Name, char>("initial", |name| {
        name.0.chars().next().unwrap_or_default()
    });

    let hero = world
        .spawn(Name("hero".to_string()))
        .expect("spawn should succeed");
    let helper = world
        .spawn(Name("healer".to_string()))
        .expect("spawn should succeed");
    let shared_world: &World = &world;

    assert_eq!(
        shared_world.find_entity_by_index::<Name, String>(&"hero".to_string()),
        Some(hero),
    );
    assert_eq!(
        shared_world.find_entities_by_index_named::<Name, char>("initial", &'h'),
        vec![hero, helper],
    );
    assert_eq!(
        shared_world.find_component_by_index::<Name, String>(&"healer".to_string()),
        Some(&Name("healer".to_string())),
    );
}

#[test]
fn secondary_index_helpers_and_component_change_ticks_work() {
    let mut world = World::new();
    world.ensure_component_index::<Name, String>(|name| name.0.clone());
    world.ensure_component_index_named::<Name, char>("initial", |name| {
        name.0.chars().next().unwrap_or_default()
    });

    let hero = world
        .spawn((Name("hero".to_string()), Health(10)))
        .expect("spawn should succeed");
    let helper = world
        .spawn((Name("healer".to_string()), Health(7)))
        .expect("spawn should succeed");
    let villain = world
        .spawn((Name("villain".to_string()), Health(9)))
        .expect("spawn should succeed");

    assert_eq!(
        world.find_entities_by_index_named::<Name, char>("initial", &'h'),
        vec![hero, helper]
    );
    assert_eq!(
        world.find_component_by_index::<Name, String>(&"villain".to_string()),
        Some(&Name("villain".to_string()))
    );

    let start = world.current_change_tick();
    world.require_mut::<Name>(hero).unwrap().0 = "hunter".to_string();
    world.despawn(villain).unwrap();

    assert!(world.component_changed_since::<Name>(start));
    assert!(world.component_changed_since::<Health>(start));
}

#[test]
fn query_support_matrix_required_forms_work() {
    let mut world = World::new();
    let e1 = world
        .spawn((A(1), B(10), C(100), Player))
        .expect("spawn should succeed");
    let e2 = world.spawn((A(2), C(200))).expect("spawn should succeed");
    let e3 = world.spawn(B(30)).expect("spawn should succeed");

    let q_read = world.query_state::<&A, ()>();
    assert_eq!(
        q_read.iter(&world).map(|a| a.0).collect::<Vec<_>>(),
        vec![1, 2]
    );

    let q_entity_mut = world.query_state::<(Entity, &mut B), ()>();
    for (entity, b) in q_entity_mut.iter(&mut world) {
        if entity == e1 {
            b.0 += 1;
        } else if entity == e3 {
            b.0 += 2;
        }
    }

    let q_mut_read = world.query_state::<(&mut A, &B), ()>();
    for (a, b) in q_mut_read.iter(&mut world) {
        a.0 += b.0;
    }

    let q_read_mut = world.query_state::<(&A, &mut C), ()>();
    for (a, c) in q_read_mut.iter(&mut world) {
        c.0 += a.0;
    }

    let q_double_mut = world.query_state::<(&mut A, &mut C), ()>();
    for (a, c) in q_double_mut.iter(&mut world) {
        a.0 += 1;
        c.0 += 1;
    }

    let q_opt_read = world.query_state::<Option<&B>, ()>();
    let opt_read: Vec<_> = q_opt_read
        .iter(&world)
        .map(|b| b.map(|value| value.0))
        .collect();
    assert_eq!(opt_read, vec![Some(11), None, Some(32)]);

    let q_opt_mut = world.query_state::<Option<&mut B>, ()>();
    for b in q_opt_mut.iter(&mut world).flatten() {
        b.0 += 10;
    }

    let q_mut_opt = world.query_state::<(&mut A, Option<&B>), ()>();
    for (a, maybe_b) in q_mut_opt.iter(&mut world) {
        if let Some(b) = maybe_b {
            a.0 += b.0;
        }
    }

    let q_entity_opt = world.query_state::<(Entity, Option<&A>), ()>();
    let entity_optional_a: Vec<_> = q_entity_opt
        .iter(&world)
        .map(|(entity, a)| (entity, a.map(|value| value.0)))
        .collect();
    assert_eq!(
        entity_optional_a,
        vec![(e1, Some(34)), (e2, Some(3)), (e3, None)]
    );

    let q_three_read = world.query_state::<(&A, &B, &C), ()>().with::<Player>();
    let three_read: Vec<_> = q_three_read
        .iter(&world)
        .map(|(a, b, c)| (a.0, b.0, c.0))
        .collect();
    assert_eq!(three_read, vec![(34, 21, 113)]);

    let q_three_mut = world.query_state::<(&mut A, &B, &C), ()>().with::<Player>();
    for (a, b, c) in q_three_mut.iter(&mut world) {
        a.0 += b.0 + c.0;
    }

    let q_three_mixed = world
        .query_state::<(&mut A, &mut C, &B), ()>()
        .with::<Player>();
    for (a, c, b) in q_three_mixed.iter(&mut world) {
        a.0 += b.0;
        c.0 += b.0;
    }

    assert_eq!(world.require::<A>(e1).unwrap().0, 189);
    assert_eq!(world.require::<B>(e1).unwrap().0, 21);
    assert_eq!(world.require::<C>(e1).unwrap().0, 134);
}

#[test]
fn query_optional_symmetry_forms_work() {
    let mut world = World::new();
    let with_b = world.spawn((A(1), B(10))).expect("spawn should succeed");
    let without_b = world.spawn(A(2)).expect("spawn should succeed");

    let read_optional = world.query_state::<(&A, Option<&B>), ()>();
    let values: Vec<_> = read_optional
        .iter(&world)
        .map(|(a, b)| (a.0, b.map(|value| value.0)))
        .collect();
    assert_eq!(values, vec![(1, Some(10)), (2, None)]);

    let read_optional_mut = world.query_state::<(&A, Option<&mut B>), ()>();
    for (a, maybe_b) in read_optional_mut.iter(&mut world) {
        if let Some(b) = maybe_b {
            b.0 += a.0;
        }
    }

    let mut_optional_mut = world.query_state::<(&mut A, Option<&mut B>), ()>();
    for (a, maybe_b) in mut_optional_mut.iter(&mut world) {
        a.0 += 1;
        if let Some(b) = maybe_b {
            b.0 += a.0;
        }
    }

    assert_eq!(world.require::<A>(with_b).unwrap().0, 2);
    assert_eq!(world.require::<A>(without_b).unwrap().0, 3);
    assert_eq!(world.require::<B>(with_b).unwrap().0, 13);
}

#[test]
fn changed_and_added_filters_work_and_compose() {
    let mut world = World::new();
    let active = world
        .spawn((Position { x: 1.0, y: 1.0 }, Player))
        .expect("spawn should succeed");
    let inactive = world
        .spawn((Position { x: 5.0, y: 5.0 }, Player, Disabled))
        .expect("spawn should succeed");

    let changed_active = world
        .query_state::<(Entity, &Position), (Changed<Position>, With<Player>, Without<Disabled>)>();
    let first_pass: Vec<_> = changed_active
        .iter(&world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(first_pass, vec![active]);
    assert!(changed_active.iter(&world).next().is_none());

    world.require_mut::<Position>(inactive).unwrap().x += 1.0;
    assert!(changed_active.iter(&world).next().is_none());

    world.require_mut::<Position>(active).unwrap().x += 1.0;
    let second_pass: Vec<_> = changed_active
        .iter(&world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(second_pass, vec![active]);

    let added_visible =
        world.query_state::<(Entity, &Health), (Added<Health>, Without<Disabled>)>();
    assert!(added_visible.iter(&world).next().is_none());

    let visible_health = world
        .spawn((Health(10), Player))
        .expect("spawn should succeed");
    let _hidden_health = world
        .spawn((Health(20), Player, Disabled))
        .expect("spawn should succeed");

    let added_pass: Vec<_> = added_visible
        .iter(&world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(added_pass, vec![visible_health]);
    assert!(added_visible.iter(&world).next().is_none());

    assert!(contains_type(
        added_visible.access().component_reads(),
        TypeId::of::<Health>(),
    ));
}

#[test]
fn query_filter_tuple_composition_works() {
    let mut world = World::new();
    let included = world
        .spawn((Position { x: 1.0, y: 1.0 }, Player))
        .expect("spawn should succeed");
    let _excluded = world
        .spawn((Position { x: 2.0, y: 2.0 }, Player, Disabled))
        .expect("spawn should succeed");

    let query = world.query_state::<(Entity, &Position), ()>();
    let seen: Vec<_> = query
        .with::<Player>()
        .without::<Disabled>()
        .iter(&world)
        .map(|(entity, _)| entity)
        .collect();
    assert_eq!(seen, vec![included]);
}

#[test]
fn broad_query_state_reuse_tracks_current_entities() {
    let mut world = World::new();
    let first = world.spawn(A(1)).expect("spawn should succeed");
    let second = world.spawn(A(2)).expect("spawn should succeed");

    let query = world.query_state::<(Entity, &A), ()>();
    let first_pass: Vec<_> = query
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    assert_eq!(first_pass, vec![(first, 1), (second, 2)]);

    world.despawn(first).unwrap();
    let third = world.spawn(A(3)).expect("spawn should succeed");

    let second_pass: Vec<_> = query
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    assert_eq!(second_pass.len(), 2);
    assert!(second_pass.contains(&(second, 2)));
    assert!(second_pass.contains(&(third, 3)));
}

#[test]
fn broad_without_filter_reuse_stays_correct_after_component_toggle() {
    let mut world = World::new();
    let enabled = world.spawn(A(1)).expect("spawn should succeed");
    let muted = world.spawn((A(2), Disabled)).expect("spawn should succeed");

    let query = world
        .query_state::<(Entity, &A), ()>()
        .without::<Disabled>();
    let initial: Vec<_> = query
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    assert_eq!(initial, vec![(enabled, 1)]);

    world.insert(enabled, Disabled).unwrap();
    world.remove::<Disabled>(muted).unwrap();

    let after_toggle: Vec<_> = query
        .iter(&world)
        .map(|(entity, a)| (entity, a.0))
        .collect();
    assert_eq!(after_toggle, vec![(muted, 2)]);
}

#[test]
fn query_state_cache_rebinds_when_iterating_a_different_world() {
    let mut first_world = World::new();
    let first_entity = first_world.spawn(A(1)).expect("spawn should succeed");
    let query = first_world.query_state::<&mut A, ()>();
    for value in query.iter(&mut first_world) {
        value.0 += 1;
    }
    assert_eq!(first_world.require::<A>(first_entity).unwrap().0, 2);

    let mut second_world = World::new();
    let second_entity = second_world.spawn(A(10)).expect("spawn should succeed");
    for value in query.iter(&mut second_world) {
        value.0 += 5;
    }

    assert_eq!(second_world.require::<A>(second_entity).unwrap().0, 15);
    assert_eq!(first_world.require::<A>(first_entity).unwrap().0, 2);
}

#[test]
fn query_state_cache_recovers_when_store_appears_after_empty_run() {
    let mut world = World::new();
    let query = world.query_state::<&mut A, ()>();
    assert!(query.iter(&mut world).next().is_none());

    let entity = world.spawn(A(4)).expect("spawn should succeed");
    for value in query.iter(&mut world) {
        value.0 += 3;
    }

    assert_eq!(world.require::<A>(entity).unwrap().0, 7);
}

#[test]
fn query_get_respects_filters_and_changed_semantics() {
    let mut world = World::new();
    let visible = world
        .spawn((Position { x: 1.0, y: 1.0 }, Player))
        .expect("spawn should succeed");
    let hidden = world
        .spawn((Position { x: 2.0, y: 2.0 }, Player, Disabled))
        .expect("spawn should succeed");

    let visible_query = world.query_state::<&Position, (With<Player>, Without<Disabled>)>();
    assert!(visible_query.get(&world, visible).is_some());
    assert!(visible_query.get(&world, hidden).is_none());

    let changed_visible =
        world.query_state::<&Position, (Changed<Position>, With<Player>, Without<Disabled>)>();
    assert!(changed_visible.get(&world, visible).is_some());
    assert!(changed_visible.get(&world, visible).is_none());

    world.require_mut::<Position>(visible).unwrap().x += 1.0;
    assert!(changed_visible.get(&world, visible).is_some());
}

#[test]
fn changed_and_added_filters_handle_remove_then_reinsert() {
    let mut world = World::new();
    let entity = world
        .spawn((Health(10), Player))
        .expect("spawn should succeed");

    let added = world.query_state::<(Entity, &Health), Added<Health>>();
    assert_eq!(
        added
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(added.iter(&world).next().is_none());

    world.remove::<Health>(entity).unwrap();
    assert!(added.iter(&world).next().is_none());

    world.insert(entity, Health(20)).unwrap();
    assert_eq!(
        added
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
}

#[test]
fn get_mut_and_require_mut_update_changed_tracking_semantics() {
    let mut world = World::new();
    let entity = world.spawn(Health(10)).expect("spawn should succeed");
    let changed = world.query_state::<(Entity, &Health), Changed<Health>>();

    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed.iter(&world).next().is_none());

    let (_, changed_before_get_mut) = world
        .__entity_component_ticks::<Health>(entity)
        .expect("health ticks should exist");
    world
        .get_mut::<Health>(entity)
        .expect("component should be available")
        .0 += 1;
    let (_, changed_after_get_mut) = world
        .__entity_component_ticks::<Health>(entity)
        .expect("health ticks should exist");
    assert!(changed_after_get_mut > changed_before_get_mut);
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
    assert!(changed.iter(&world).next().is_none());

    let (_, changed_before_require_mut) = world
        .__entity_component_ticks::<Health>(entity)
        .expect("health ticks should exist");
    world
        .require_mut::<Health>(entity)
        .expect("component should be available")
        .0 += 1;
    let (_, changed_after_require_mut) = world
        .__entity_component_ticks::<Health>(entity)
        .expect("health ticks should exist");
    assert!(changed_after_require_mut > changed_before_require_mut);
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
}

#[test]
fn failed_mutable_component_lookup_does_not_create_mutation_facts() {
    let mut world = World::new();
    let entity = world.spawn(Player).expect("spawn should succeed");
    let changed = world.query_state::<(Entity, &Health), Changed<Health>>();
    let before = world.current_change_tick();

    assert!(world.get_mut::<Health>(entity).is_none());
    assert!(matches!(
        world.require_mut::<Health>(entity),
        Err(EntityError::MissingComponent { .. })
    ));

    assert_eq!(world.current_change_tick(), before);
    assert!(!world.component_changed_since::<Health>(before));
    assert!(changed.iter(&world).next().is_none());
}

#[test]
fn insert_remove_and_despawn_keep_change_ticks_in_sync() {
    let mut world = World::new();
    let start = world.current_change_tick();
    let entity = world.spawn(Player).expect("spawn should succeed");

    world.insert(entity, Health(10)).unwrap();
    let _: Health = world.remove(entity).unwrap();
    world.insert(entity, Health(20)).unwrap();
    world.despawn(entity).unwrap();

    assert!(world.component_changed_since::<Health>(start));
}

#[test]
fn component_index_rebuild_remains_correct_under_churn() {
    let mut world = World::new();
    world.ensure_component_index::<Name, String>(|name| name.0.clone());

    let first = world
        .spawn(Name("alpha".to_string()))
        .expect("spawn should succeed");
    let second = world
        .spawn(Name("beta".to_string()))
        .expect("spawn should succeed");
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"alpha".to_string()),
        Some(first)
    );
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"beta".to_string()),
        Some(second)
    );

    let _: Name = world.remove(first).unwrap();
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"alpha".to_string()),
        None
    );

    world.insert(first, Name("gamma".to_string())).unwrap();
    let third = world
        .spawn(Name("alpha".to_string()))
        .expect("spawn should succeed");
    world.despawn(second).unwrap();

    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"beta".to_string()),
        None
    );
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"gamma".to_string()),
        Some(first)
    );
    assert_eq!(
        world.find_entity_by_index::<Name, String>(&"alpha".to_string()),
        Some(third)
    );

    world.require_mut::<Name>(first).unwrap().0 = "alpha".to_string();
    let alpha = world.find_entities_by_index::<Name, String>(&"alpha".to_string());
    assert_eq!(alpha.len(), 2);
    assert!(alpha.contains(&first));
    assert!(alpha.contains(&third));
}

#[test]
fn command_queued_spawn_is_visible_next_stage_and_not_readded_next_frame() {
    fn queue_spawn_once(mut gate: ResMut<SpawnGate>, mut commands: Commands) {
        if gate.0 {
            return;
        }
        commands.spawn(Health(1));
        gate.0 = true;
    }

    fn observe_added(
        mut query: Query<&Health, Added<Health>>,
        mut seen: ResMut<AddedHealthCounts>,
    ) {
        seen.0.push(query.iter().count());
    }

    let mut world = World::new();
    world.insert_resource(SpawnGate(false));
    world.insert_resource(AddedHealthCounts(Vec::new()));

    let mut runtime = Runtime::new();
    runtime.add_systems::<WorldUpdate, _, _>(&mut world, queue_spawn_once.in_set(SpawnStage));
    runtime.add_systems::<WorldUpdate, _, _>(
        &mut world,
        observe_added.in_set(ObserveStage).after(SpawnStage),
    );

    runtime.run_schedule::<WorldUpdate>(&mut world).unwrap();
    runtime.run_schedule::<WorldUpdate>(&mut world).unwrap();

    assert_eq!(world.resource::<AddedHealthCounts>().unwrap().0, vec![1, 0]);
}

#[test]
fn system_param_access_metadata_reports_expected_sets() {
    world_for_param_access_checks();
}

fn world_for_param_access_checks() {
    let mut world = World::new();
    world.insert_resource(Frame(0));

    let query_state =
        <Query<'static, 'static, (&mut Position, &Velocity)> as SystemParam>::init_state(
            &mut world,
        )
        .unwrap();
    let query_access =
        <Query<'static, 'static, (&mut Position, &Velocity)> as SystemParam>::access(&query_state);
    assert!(contains_type(
        query_access.component_writes(),
        TypeId::of::<Position>()
    ));
    assert!(contains_type(
        query_access.component_reads(),
        TypeId::of::<Velocity>()
    ));

    let res_access = <Res<'static, Frame> as SystemParam>::access(&());
    assert!(contains_type(
        res_access.resource_reads(),
        TypeId::of::<Frame>()
    ));

    let res_access = <Res<'static, Frame> as SystemParam>::access(&());
    assert!(contains_type(
        res_access.resource_reads(),
        TypeId::of::<Frame>()
    ));

    let res_mut_access = <ResMut<'static, Frame> as SystemParam>::access(&());
    assert!(contains_type(
        res_mut_access.resource_writes(),
        TypeId::of::<Frame>()
    ));

    let commands_access = <Commands<'static> as SystemParam>::access(&());
    assert!(commands_access.deferred_structural_mutation());
}

fn contains_type(entries: &[QueryTypeAccess], type_id: TypeId) -> bool {
    entries.iter().any(|entry| entry.type_id() == type_id)
}

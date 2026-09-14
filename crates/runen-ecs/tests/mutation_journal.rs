use runen_ecs::RuntimeError;
use runen_ecs::prelude::*;
use std::error::Error;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component)]
struct B(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component)]
struct Marker;

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Resource)]
struct R(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct Failure;

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected system failure")
    }
}

impl Error for Failure {}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct PanicPayload(u8);

#[test]
fn journal_preserves_noop_mutation_multiplicity_and_mixed_parameter_semantics() {
    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    world.insert_resource(R(2));
    let before = world.current_change_cursor();

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        move |mut query: Query<&mut A>, mut resource: ResMut<R>| {
            let _ = query.get(entity).expect("component should exist");
            let _ = query.get(entity).expect("component should exist");
            let _: &mut R = &mut resource;
            let _: &mut R = &mut resource;
        },
    );

    runtime
        .run_schedule::<Update>(&mut world)
        .expect("system should succeed");

    assert_eq!(world.require::<A>(entity).unwrap().0, 1);
    assert_eq!(world.resource::<R>().unwrap().0, 2);
    assert_eq!(
        world.current_change_cursor().tick(),
        before.tick() + 4,
        "each mutable exposure must retain its own cursor event"
    );
    assert!(world.component_changed_since::<A>(before).unwrap());
    assert!(world.resource_changed_since::<R>(before).unwrap());

    let changed = world.query_state::<(Entity, &A), Changed<A>>();
    assert_eq!(
        changed
            .iter(&world)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![entity]
    );
}

#[test]
fn journal_reconciles_admitted_events_when_system_returns_err() {
    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    world.insert_resource(R(2));
    let before = world.current_change_cursor();

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        move |mut query: Query<&mut A>, mut resource: ResMut<R>| {
            query.get(entity).expect("component should exist").0 = 7;
            resource.0 = 9;
            Err::<(), _>(Failure)
        },
    );

    let result = runtime.run_schedule::<Update>(&mut world);
    assert!(matches!(result, Err(RuntimeError::System { .. })));
    assert_eq!(world.require::<A>(entity).unwrap().0, 7);
    assert_eq!(world.resource::<R>().unwrap().0, 9);
    assert_eq!(world.current_change_cursor().tick(), before.tick() + 2);
    assert!(world.component_changed_since::<A>(before).unwrap());
    assert!(world.resource_changed_since::<R>(before).unwrap());
}

#[test]
fn journal_reconciles_before_resuming_original_user_panic() {
    let mut world = World::new();
    let entity = world.spawn(A(1)).expect("spawn should succeed");
    let before = world.current_change_cursor();

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(
        &mut world,
        (move |mut query: Query<&mut A>, mut commands: Commands| -> () {
            query.get(entity).expect("component should exist").0 = 7;
            commands.spawn(Marker);
            std::panic::panic_any(PanicPayload(7));
        })
        .on_invoker_thread(),
    );

    let payload = catch_unwind(AssertUnwindSafe(|| {
        runtime.run_schedule::<Update>(&mut world).unwrap();
    }))
    .expect_err("the user panic must propagate");
    assert_eq!(
        payload.downcast_ref::<PanicPayload>(),
        Some(&PanicPayload(7))
    );
    assert_eq!(world.require::<A>(entity).unwrap().0, 7);
    assert_eq!(world.current_change_cursor().tick(), before.tick() + 1);
    assert!(world.component_changed_since::<A>(before).unwrap());
    assert_eq!(world.query_state::<&Marker, ()>().iter(&world).count(), 0);
}

#[test]
fn mutable_tuple_journal_keeps_both_payloads_and_events() {
    let mut world = World::new();
    let entity = world.spawn((A(1), B(2))).expect("spawn should succeed");
    let before = world.current_change_cursor();

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, move |mut query: Query<(&mut A, &mut B)>| {
        let (a, b) = query.get(entity).expect("components should exist");
        a.0 = 3;
        b.0 = 4;
    });
    runtime
        .run_schedule::<Update>(&mut world)
        .expect("system should succeed");

    assert_eq!(world.require::<A>(entity).unwrap().0, 3);
    assert_eq!(world.require::<B>(entity).unwrap().0, 4);
    assert_eq!(world.current_change_cursor().tick(), before.tick() + 2);
    assert!(world.component_changed_since::<A>(before).unwrap());
    assert!(world.component_changed_since::<B>(before).unwrap());
}

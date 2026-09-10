use runen_ecs::prelude::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct A(i32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, runen_ecs::Component, runen_ecs::Resource)]
struct B(i32);

#[test]
fn same_type_double_mut_query_is_rejected_before_iteration() {
    let mut world = World::new();
    world.spawn(A(1)).expect("spawn should succeed");

    let panic_result = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.query_state::<(&mut A, &mut A), ()>();
    }));
    assert!(panic_result.is_err());
}

#[test]
fn same_type_mut_read_query_is_rejected_before_iteration() {
    let mut world = World::new();
    world.spawn(A(1)).expect("spawn should succeed");

    let panic_result = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.query_state::<(&mut A, &A), ()>();
    }));
    assert!(panic_result.is_err());
}

#[test]
fn same_type_optional_mut_query_is_rejected_before_iteration() {
    let mut world = World::new();
    world.spawn(A(1)).expect("spawn should succeed");

    let panic_result = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.query_state::<(&mut A, Option<&mut A>), ()>();
    }));
    assert!(panic_result.is_err());
}

#[test]
fn query_state_rebinds_world_scope_and_resets_change_cursor() {
    let mut first = World::new();
    let first_entity = first.spawn(A(1)).expect("spawn should succeed");
    let changed = first.query_state::<(Entity, &A), Changed<A>>();

    assert_eq!(
        changed
            .iter(&first)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![first_entity]
    );
    assert!(changed.iter(&first).next().is_none());

    let mut second = World::new();
    let second_entity = second.spawn(A(10)).expect("spawn should succeed");
    assert_eq!(
        changed
            .iter(&second)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![second_entity],
        "rebinding must reset the change cursor instead of carrying another World's tick"
    );
    assert!(changed.iter(&second).next().is_none());

    assert_eq!(
        changed
            .iter(&first)
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>(),
        vec![first_entity],
        "rebinding back must reset world-local query cursor state again"
    );
}

#[test]
fn optional_mut_query_handles_present_absent_and_repeated_iteration() {
    let mut world = World::new();
    let with_b = world.spawn((A(1), B(10))).expect("spawn should succeed");
    let without_b = world.spawn(A(2)).expect("spawn should succeed");

    let query = world.query_state::<(&mut A, Option<&mut B>), ()>();

    for (a, maybe_b) in query.iter(&mut world) {
        a.0 += 1;
        if let Some(b) = maybe_b {
            b.0 += a.0;
        }
    }

    for (a, maybe_b) in query.iter(&mut world) {
        a.0 += 1;
        if let Some(b) = maybe_b {
            b.0 += a.0;
        }
    }

    assert_eq!(world.require::<A>(with_b).unwrap().0, 3);
    assert_eq!(world.require::<A>(without_b).unwrap().0, 4);
    assert_eq!(world.require::<B>(with_b).unwrap().0, 15);
}

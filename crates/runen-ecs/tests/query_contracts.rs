use runen_ecs::prelude::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Component)]
struct Position;

fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        message
    } else {
        "<non-string panic>"
    }
}

#[test]
fn invalid_direct_query_reports_conflicting_component_borrow() {
    let world = World::new();

    let payload = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.query::<(&mut Position, &Position)>();
    }))
    .expect_err("overlapping mutable/shared direct query must panic during construction");

    let message = panic_message(payload.as_ref());
    assert!(message.contains("invalid query state"), "{message}");
    assert!(
        message.contains("conflicting component borrows"),
        "{message}"
    );
    assert!(message.contains("Position"), "{message}");
}

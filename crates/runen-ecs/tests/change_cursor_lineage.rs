use runen_ecs::{Added, ChangeCursorError, Changed, World};

#[derive(Debug, Copy, Clone, runen_ecs::Component)]
struct Position(i32);

#[derive(runen_ecs::Component)]
struct NeverComponent;

#[derive(runen_ecs::Resource)]
struct Frame(u32);

#[derive(runen_ecs::Resource)]
struct NeverResource;

#[test]
fn cursors_are_ordered_only_within_their_world_lineage() {
    let first = World::new();
    let second = World::new();

    let first_cursor = first.current_change_cursor();
    let second_cursor = second.current_change_cursor();

    assert_eq!(first_cursor.epoch(), second_cursor.epoch());
    assert_eq!(first_cursor.tick(), second_cursor.tick());
    assert_ne!(first_cursor, second_cursor);
    assert_eq!(first_cursor.partial_cmp(&second_cursor), None);

    let debug = format!("{first_cursor:?}");
    assert!(!debug.contains("scope"));
}

#[test]
fn foreign_cursor_is_rejected_before_change_record_lookup() {
    let first = World::new();
    let second = World::new();
    let foreign = first.current_change_cursor();

    assert_eq!(
        second.resource_changed_since::<NeverResource>(foreign),
        Err(ChangeCursorError::ForeignWorld)
    );
    assert_eq!(
        second.component_changed_since::<NeverComponent>(foreign),
        Err(ChangeCursorError::ForeignWorld)
    );
}

#[test]
fn same_world_component_and_resource_freshness_remains_ordered() {
    let mut world = World::new();
    let resource_origin = world.current_change_cursor();

    assert_eq!(
        world.resource_changed_since::<Frame>(resource_origin),
        Ok(false)
    );
    world.insert_resource(Frame(1));
    assert_eq!(
        world.resource_changed_since::<Frame>(resource_origin),
        Ok(true)
    );
    assert_eq!(world.resource::<Frame>().unwrap().0, 1);

    let component_origin = world.current_change_cursor();
    assert_eq!(
        world.component_changed_since::<Position>(component_origin),
        Ok(false)
    );
    world.spawn(Position(7)).unwrap();
    assert_eq!(
        world.component_changed_since::<Position>(component_origin),
        Ok(true)
    );
}

#[test]
fn query_change_filters_rebind_to_the_new_world_origin() {
    let mut first = World::new();
    first.spawn(Position(1)).unwrap();

    let added = first.query_filtered::<&Position, Added<Position>>();
    let changed = first.query_filtered::<&Position, Changed<Position>>();

    assert_eq!(
        added
            .iter(&first)
            .map(|position| position.0)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert!(added.iter(&first).next().is_none());
    assert_eq!(
        changed
            .iter(&first)
            .map(|position| position.0)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert!(changed.iter(&first).next().is_none());

    let mut second = World::new();
    second.spawn(Position(2)).unwrap();

    assert_eq!(
        added
            .iter(&second)
            .map(|position| position.0)
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(
        changed
            .iter(&second)
            .map(|position| position.0)
            .collect::<Vec<_>>(),
        vec![2]
    );
}

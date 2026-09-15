use runen_ecs::prelude::*;

#[derive(Debug, Component)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Debug, Component)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Debug, Component)]
struct Health(u32);

#[derive(Debug, Resource)]
struct Gravity(f32);

#[derive(Bundle)]
struct MovingBody {
    position: Position,
    velocity: Velocity,
}

fn main() {
    let mut world = World::new();
    world.insert_resource(Gravity(9.81));

    let entity = world
        .spawn(MovingBody {
            position: Position { x: 1.0, y: 2.0 },
            velocity: Velocity { x: 3.0, y: 0.0 },
        })
        .expect("moving body should spawn");

    let body = world.entity(entity).expect("entity should exist");
    assert!(body.contains::<Position>());
    assert!(body.contains::<Velocity>());
    assert_eq!(body.require::<Position>().unwrap().x, 1.0);

    {
        let mut body = world.entity_mut(entity).expect("entity should exist");
        body.require_mut::<Position>().unwrap().x += 4.0;
        body.insert(Health(100)).expect("health should insert");
    }

    assert_eq!(world.require::<Position>(entity).unwrap().x, 5.0);
    assert_eq!(world.require::<Health>(entity).unwrap().0, 100);

    let removed: Health = world
        .entity_mut(entity)
        .unwrap()
        .remove()
        .expect("health should remove");
    assert_eq!(removed.0, 100);

    world.resource_mut::<Gravity>().unwrap().0 = 3.71;
    assert_eq!(world.resource::<Gravity>().unwrap().0, 3.71);

    let velocity = world.require::<Velocity>(entity).unwrap();
    println!(
        "entity {entity:?}: position=({:.1}, {:.1}), velocity=({:.1}, {:.1}), gravity={:.2}",
        world.require::<Position>(entity).unwrap().x,
        world.require::<Position>(entity).unwrap().y,
        velocity.x,
        velocity.y,
        world.resource::<Gravity>().unwrap().0,
    );

    world.despawn(entity).expect("entity should despawn");
    assert!(world.entity(entity).is_err());
}

use runen_ecs::prelude::*;

#[derive(Debug, runen_ecs::Component)]
struct Position(i32);

#[derive(Debug, runen_ecs::Component)]
struct Velocity(i32);

#[derive(Debug, runen_ecs::Component)]
struct Name(&'static str);

#[derive(Debug, runen_ecs::Component)]
struct Active;

#[derive(Debug, runen_ecs::Component)]
struct Sleeping;

#[derive(Debug, runen_ecs::Component)]
struct Player;

fn main() {
    let mut world = World::new();
    let player = world
        .spawn((Position(0), Velocity(2), Name("player"), Active, Player))
        .unwrap();
    let enemy = world
        .spawn((Position(10), Velocity(-1), Active))
        .unwrap();
    let sleeping = world
        .spawn((Position(20), Velocity(5), Active, Sleeping))
        .unwrap();

    let moving = world
        .query_state::<(&mut Position, &Velocity), ()>()
        .with::<Active>()
        .without::<Sleeping>();
    for (position, velocity) in moving.iter(&mut world) {
        position.0 += velocity.0;
    }

    let positions = world.query_state::<(Entity, &Position), ()>();
    assert_eq!(positions.get(&world, player).unwrap().1.0, 2);
    assert_eq!(positions.get(&world, enemy).unwrap().1.0, 9);
    assert_eq!(positions.get(&world, sleeping).unwrap().1.0, 20);

    let player_position = world
        .query_state::<&Position, With<Player>>()
        .single(&world)
        .expect("exactly one player should exist");
    assert_eq!(player_position.0, 2);

    let names = world
        .query_state::<(Entity, Option<&Name>), ()>()
        .with::<Position>();
    let mut named: Vec<_> = names
        .iter(&world)
        .map(|(entity, name)| (entity, name.map(|name| name.0)))
        .collect();
    // Query iteration order is not a public contract; sort only for presentation.
    named.sort_unstable_by_key(|(entity, _)| *entity);

    assert!(named.contains(&(player, Some("player"))));
    assert!(named.contains(&(enemy, None)));
    assert!(named.contains(&(sleeping, None)));

    println!("queried {} positioned entities", named.len());
}

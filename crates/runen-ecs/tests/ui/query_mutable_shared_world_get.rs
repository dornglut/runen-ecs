#[derive(runen_ecs::Component)]
struct Position;

fn main() {
    let mut world = runen_ecs::World::new();
    let entity = world.spawn(Position).unwrap();
    let state = world.query_state::<&mut Position, ()>();
    let _item = state.get(&world, entity);
}

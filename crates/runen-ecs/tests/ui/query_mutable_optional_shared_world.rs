#[derive(runen_ecs::Component)]
struct Position;

fn main() {
    let world = runen_ecs::World::new();
    let state = world.query_state::<Option<&mut Position>, ()>();
    let _items = state.iter(&world);
}

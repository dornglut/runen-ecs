#[derive(runen_ecs::Component)]
struct Position(i32);

fn main() {
    let world = runen_ecs::World::new();
    let state = world.query::<&mut Position>();
    let _items = state.iter(&world);
}

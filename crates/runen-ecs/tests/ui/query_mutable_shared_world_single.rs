#[derive(runen_ecs::Component)]
struct Position;

fn main() {
    let world = runen_ecs::World::new();
    let state = world.query_state::<&mut Position, ()>();
    let _item = state.single(&world);
}

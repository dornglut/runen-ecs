#[derive(runen_ecs::Component)]
struct A;

#[derive(runen_ecs::Component)]
struct B;

fn main() {
    let world = runen_ecs::World::new();
    let state = world.query_state::<(&mut A, &B), ()>();
    let _items = state.iter(&world);
}

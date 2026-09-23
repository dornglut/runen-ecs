use runen_ecs::{Component, World};

#[derive(Component)]
struct A;

fn main() {
    let mut world = World::new();
    world.spawn(A).unwrap();
    let query = world.query::<&mut A>();
    let segments = query.try_contiguous_segments(&mut world).unwrap();
    world.spawn(A).unwrap();
    drop(segments);
}

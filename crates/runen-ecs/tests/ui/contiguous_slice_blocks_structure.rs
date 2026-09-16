use runen_ecs::{Component, World};

#[derive(Component)]
struct A;

fn main() {
    let mut world = World::new();
    world.spawn(A).unwrap();
    let query = world.query::<&A>();
    let mut segments = query.try_contiguous_segments(&world).unwrap();
    let segment = segments.next().unwrap();
    let values = segment.component::<A>().unwrap();
    drop(segments);
    drop(segment);
    world.spawn(A).unwrap();
    std::hint::black_box(values);
}

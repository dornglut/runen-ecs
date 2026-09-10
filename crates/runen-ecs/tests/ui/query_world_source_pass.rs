#[derive(runen_ecs::Component)]
struct A;

#[derive(runen_ecs::Component)]
struct B;

fn main() {
    let mut world = runen_ecs::World::new();
    let entity = world.spawn((A, B)).unwrap();

    let shared = world.query_state::<&A, ()>();
    let _ = shared.iter(&world).count();
    let _ = shared.get(&world, entity);
    let _ = shared.single(&world);

    let shared_shapes = [
        world.query_state::<(runen_ecs::Entity, &A), ()>().iter(&world).count(),
        world.query_state::<(&A, &B), ()>().iter(&world).count(),
        world.query_state::<Option<&A>, ()>().iter(&world).count(),
        world
            .query_state::<(&A, Option<&B>), ()>()
            .iter(&world)
            .count(),
        world
            .query_state::<(runen_ecs::Entity, Option<&A>), ()>()
            .iter(&world)
            .count(),
        world
            .query_state::<(&A, &B, &A), ()>()
            .iter(&world)
            .count(),
    ];
    let _ = shared_shapes;

    let mutable = world.query_state::<&mut A, ()>();
    {
        let _ = mutable.iter(&mut world).count();
    }
    {
        let _ = mutable.get(&mut world, entity);
    }
    {
        let _ = mutable.single(&mut world);
    }

    let tuple = world.query_state::<(&mut A, &mut B), ()>();
    let _ = tuple.iter(&mut world).count();
    let optional = world.query_state::<Option<&mut A>, ()>();
    let _ = optional.iter(&mut world).count();
}

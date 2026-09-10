#[derive(runen_ecs::Component, runen_ecs::Reflect)]
struct Component {
    value: u32,
}

#[derive(runen_ecs::Resource, runen_ecs::Reflect)]
struct Resource {
    value: u32,
}

fn main() {
    let mut world = runen_ecs::World::new();
    world.register_component_type::<Component>();
    world.register_resource_type::<Resource>();
    world.insert_registered_resource(Resource { value: 1 });
}

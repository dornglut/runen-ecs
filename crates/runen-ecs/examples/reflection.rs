use runen_ecs::prelude::*;
use std::any::TypeId;

#[derive(Debug, Component, runen_ecs::Reflect)]
struct RenderSettings {
    exposure: f32,
    visible: bool,
}

fn main() {
    let mut world = World::new();
    world.register_reflected_component::<RenderSettings>();

    let entity = world
        .spawn(RenderSettings {
            exposure: 1.25,
            visible: true,
        })
        .expect("entity should spawn");
    let settings_type = TypeId::of::<RenderSettings>();

    let type_info = world
        .reflected_component_type_info(settings_type)
        .expect("registered reflected component should expose type information");
    let struct_info = type_info
        .struct_info()
        .expect("RenderSettings should be a reflected struct");
    assert_eq!(type_info.display_name, "RenderSettings");
    assert!(struct_info.field_named("exposure").is_some());
    assert!(struct_info.field_named("visible").is_some());

    {
        let reflected = world
            .reflected_component_value_ref(entity, settings_type)
            .expect("live reflected component should be readable");
        let fields = reflected
            .struct_ref()
            .expect("RenderSettings should expose reflected fields");
        let exposure = fields
            .field("exposure")
            .expect("exposure field should exist")
            .downcast_ref::<f32>()
            .expect("exposure should be f32");
        assert_eq!(*exposure, 1.25);
    }

    {
        let reflected = world
            .reflected_component_value_mut(entity, settings_type)
            .expect("live reflected component should be mutable");
        let mut fields = reflected
            .struct_mut()
            .expect("RenderSettings should expose mutable reflected fields");
        let mut exposure_value = fields
            .field_mut("exposure")
            .expect("exposure field should exist");
        let exposure = exposure_value
            .downcast_mut::<f32>()
            .expect("exposure should be f32");
        *exposure = 2.0;
    }

    assert_eq!(
        world
            .get::<RenderSettings>(entity)
            .expect("component should remain live")
            .exposure,
        2.0
    );

    println!(
        "reflected {} fields and updated exposure to 2.0",
        struct_info.field_count()
    );
}

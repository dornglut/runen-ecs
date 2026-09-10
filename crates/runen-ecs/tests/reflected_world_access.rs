use runen_ecs::prelude::*;
use std::any::TypeId;

#[derive(Debug, Clone, runen_ecs::Reflect)]
struct Vec2 {
    x: f32,
    y: f32,
}

#[derive(Debug, Clone, runen_ecs::Component, runen_ecs::Reflect)]
struct Position {
    value: Vec2,
    speed: f32,
}

#[derive(Debug, Clone, runen_ecs::Resource, runen_ecs::Reflect)]
struct CameraSettings {
    zoom: f32,
    exposure: f32,
}

#[test]
fn reflected_component_value_ref_reads_live_component() {
    let mut world = World::new();
    world.register_reflected_component::<Position>();

    let entity = world
        .spawn(Position {
            value: Vec2 { x: 1.0, y: 2.0 },
            speed: 3.5,
        })
        .expect("spawn should succeed");

    let reflected = world
        .reflected_component_value_ref(entity, TypeId::of::<Position>())
        .expect("Position reflected component value should exist");

    let struct_ref = reflected
        .struct_ref()
        .expect("Position should be struct-reflectable");

    let speed = struct_ref
        .field("speed")
        .expect("speed field should exist")
        .downcast_ref::<f32>()
        .expect("speed should be f32");

    assert_eq!(*speed, 3.5);
}

#[test]
fn reflected_component_value_mut_updates_live_component() {
    let mut world = World::new();
    world.register_reflected_component::<Position>();

    let entity = world
        .spawn(Position {
            value: Vec2 { x: 1.0, y: 2.0 },
            speed: 3.5,
        })
        .expect("spawn should succeed");

    let reflected = world
        .reflected_component_value_mut(entity, TypeId::of::<Position>())
        .expect("Position reflected component value should exist");

    let mut struct_mut = reflected
        .struct_mut()
        .expect("Position should be struct-reflectable");

    let mut speed_value = struct_mut
        .field_mut("speed")
        .expect("speed field should exist");

    let speed = speed_value
        .downcast_mut::<f32>()
        .expect("speed should be f32");

    *speed = 7.0;

    assert_eq!(
        world
            .get::<Position>(entity)
            .expect("component should still exist")
            .speed,
        7.0
    );
}

#[test]
fn reflected_resource_value_ref_reads_live_resource() {
    let mut world = World::new();
    world.register_reflected_resource::<CameraSettings>();
    world.insert_resource(CameraSettings {
        zoom: 2.0,
        exposure: 1.25,
    });

    let reflected = world
        .reflected_resource_value_ref(TypeId::of::<CameraSettings>())
        .expect("CameraSettings reflected resource value should exist");

    let struct_ref = reflected
        .struct_ref()
        .expect("CameraSettings should be struct-reflectable");

    let zoom = struct_ref
        .field("zoom")
        .expect("zoom field should exist")
        .downcast_ref::<f32>()
        .expect("zoom should be f32");

    assert_eq!(*zoom, 2.0);
}

#[test]
fn reflected_resource_value_mut_updates_live_resource() {
    let mut world = World::new();
    world.register_reflected_resource::<CameraSettings>();
    world.insert_resource(CameraSettings {
        zoom: 2.0,
        exposure: 1.25,
    });

    let reflected = world
        .reflected_resource_value_mut(TypeId::of::<CameraSettings>())
        .expect("CameraSettings reflected resource value should exist");

    let mut struct_mut = reflected
        .struct_mut()
        .expect("CameraSettings should be struct-reflectable");

    let mut exposure_value = struct_mut
        .field_mut("exposure")
        .expect("exposure field should exist");

    let exposure = exposure_value
        .downcast_mut::<f32>()
        .expect("exposure should be f32");

    *exposure = 2.5;

    assert_eq!(
        world
            .resource::<CameraSettings>()
            .expect("resource should still exist")
            .exposure,
        2.5
    );
}

#[test]
fn entity_component_introspection_reports_registered_component() {
    let mut world = World::new();
    world.register_reflected_component::<Position>();

    let entity = world
        .spawn(Position {
            value: Vec2 { x: 0.0, y: 0.0 },
            speed: 1.0,
        })
        .expect("spawn should succeed");

    let type_id = TypeId::of::<Position>();

    assert!(world.entity_has_component_type(entity, type_id));
    assert_eq!(world.entity_component_count(entity), 1);

    let component_type_ids = world.entity_component_type_ids(entity);
    assert_eq!(component_type_ids, vec![type_id]);

    let _component_type = world
        .reflected_component_type_info(type_id)
        .expect("type info should exist");
}

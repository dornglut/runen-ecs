use runen_ecs::Reflect;

fn assert_reflect<T: runen_ecs::reflect::Reflect>() {}

#[test]
fn position_implements_reflect() {
    assert_reflect::<Position>();
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, runen_ecs::Reflect)]
enum TextureFilter {
    Nearest,
    Linear,
}

#[test]
fn reflected_named_struct_exposes_fields() {
    let mut registry = runen_ecs::TypeRegistry::new();
    let info = registry.register::<Position>();

    let struct_info = info.struct_info().expect("Position should be a struct");
    assert_eq!(struct_info.field_count(), 2);
    assert_eq!(struct_info.field_at(0).unwrap().name, "value");
    assert_eq!(struct_info.field_at(1).unwrap().name, "speed");
}

#[test]
fn reflected_value_supports_field_lookup() {
    let position = Position {
        value: Vec2 { x: 1.0, y: 2.0 },
        speed: 3.5,
    };

    let reflected = position.reflect_ref();
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
fn reflected_value_supports_field_mutation() {
    let mut position = Position {
        value: Vec2 { x: 1.0, y: 2.0 },
        speed: 3.5,
    };

    let reflected = position.reflect_mut();
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

    assert_eq!(position.speed, 7.0);
}

#[test]
fn reflected_unit_enum_exposes_variants_and_current_symbol() {
    let mut registry = runen_ecs::TypeRegistry::new();
    let info = registry.register::<TextureFilter>();
    let enum_info = info.enum_info().expect("TextureFilter should be an enum");

    assert_eq!(enum_info.variant_count(), 2);
    assert!(enum_info.variant_named("Nearest").is_some());
    assert!(enum_info.variant_named("Linear").is_some());

    let filter = TextureFilter::Linear;
    let reflected = filter.reflect_ref();
    let enum_ref = reflected
        .enum_ref()
        .expect("TextureFilter should be enum-reflectable");

    assert_eq!(enum_ref.current_symbol(), Some("Linear"));
}

#[test]
fn reflected_unit_enum_supports_variant_mutation() {
    let mut filter = TextureFilter::Nearest;
    let reflected = filter.reflect_mut();
    let mut enum_mut = reflected
        .enum_mut()
        .expect("TextureFilter should be enum-reflectable");

    assert!(enum_mut.set_unit_variant("Linear"));
    assert_eq!(filter, TextureFilter::Linear);
}

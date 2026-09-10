use runen_ecs::{Reflect, TypeInfo, TypeRegistry, World};
use std::any::TypeId;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(runen_ecs::Component, runen_ecs::Reflect)]
struct ComponentA {
    value: u32,
}

#[derive(runen_ecs::Component, runen_ecs::Reflect)]
struct ComponentB {
    value: u32,
}

#[derive(runen_ecs::Component, runen_ecs::Resource, runen_ecs::Reflect)]
struct Both {
    value: u32,
}

#[derive(runen_ecs::Resource, runen_ecs::Reflect)]
struct ResourceC {
    value: u32,
}

struct SameNameA;
struct SameNameB;

impl Reflect for SameNameA {
    fn type_info() -> TypeInfo {
        TypeInfo::new("SameNameA", "same", runen_ecs::ReflectShape::Opaque)
    }
}

impl Reflect for SameNameB {
    fn type_info() -> TypeInfo {
        TypeInfo::new("SameNameB", "same", runen_ecs::ReflectShape::Opaque)
    }
}

#[derive(runen_ecs::Component)]
struct DescriptorProbe;

static DESCRIPTOR_PROBE_CALLS: AtomicUsize = AtomicUsize::new(0);

impl Reflect for DescriptorProbe {
    fn type_info() -> TypeInfo {
        let display_name = if DESCRIPTOR_PROBE_CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
            "first"
        } else {
            "later"
        };
        TypeInfo::new(
            std::any::type_name::<Self>(),
            display_name,
            runen_ecs::ReflectShape::Opaque,
        )
    }
}

#[derive(runen_ecs::Reflect)]
struct Generic<T>
where
    T: Reflect,
{
    value: T,
}

struct ConstField<const N: usize>([u8; N]);

impl<const N: usize> Reflect for ConstField<N> {
    fn type_info() -> TypeInfo {
        TypeInfo::new(
            std::any::type_name::<Self>(),
            "bytes",
            runen_ecs::ReflectShape::Opaque,
        )
    }
}

#[derive(runen_ecs::Reflect)]
struct ConstGeneric<const N: usize> {
    value: ConstField<N>,
}

fn names(types: impl Iterator<Item = TypeInfo>) -> Vec<&'static str> {
    types.map(|type_info| type_info.rust_name).collect()
}

#[test]
fn descriptor_access_is_pure_and_registry_instances_are_independent() {
    let mut first = TypeRegistry::new();
    let mut second = TypeRegistry::default();

    let _ = ComponentA::type_info();
    assert_eq!(first.types().count(), 0);
    assert_eq!(second.types().count(), 0);

    let first_info = first.register::<ComponentA>();
    let second_info = second.register::<ComponentA>();
    assert_eq!(
        first.register::<ComponentA>().rust_name,
        first_info.rust_name
    );
    assert_eq!(second_info.rust_name, first_info.rust_name);
    assert_eq!(first.types().count(), 1);
    assert_eq!(second.types().count(), 1);
    assert_eq!(names(first.types()), names(second.types()));
}

#[test]
fn registry_registration_is_ordered_and_names_are_not_identity() {
    let mut registry = TypeRegistry::new();
    registry.register::<SameNameA>();
    registry.register::<SameNameB>();

    assert_eq!(registry.types().count(), 2);
    assert_eq!(names(registry.types()), vec!["SameNameA", "SameNameB"]);
    assert_eq!(
        registry
            .get(TypeId::of::<SameNameA>())
            .unwrap()
            .display_name,
        "same"
    );
    assert_eq!(
        registry
            .get(TypeId::of::<SameNameB>())
            .unwrap()
            .display_name,
        "same"
    );
}

#[test]
fn world_role_orders_are_independent_and_share_one_metadata_registry() {
    let mut world = World::new();
    world.register_reflected_component::<ComponentB>();
    world.register_reflected_component::<ComponentA>();
    world.register_reflected_component::<Both>();
    world.register_reflected_component::<ComponentA>();
    world.register_reflected_resource::<ResourceC>();
    world.register_reflected_resource::<Both>();
    world.register_reflected_resource::<ResourceC>();

    assert_eq!(
        names(world.reflected_component_types().into_iter()),
        vec![
            "reflect_registry::ComponentB",
            "reflect_registry::ComponentA",
            "reflect_registry::Both"
        ]
    );
    assert_eq!(
        names(world.reflected_resource_types().into_iter()),
        vec!["reflect_registry::ResourceC", "reflect_registry::Both"]
    );
    assert_eq!(world.type_registry().types().count(), 4);
    assert!(world.has_reflected_resource_type(TypeId::of::<Both>()));

    let both_type_id = TypeId::of::<Both>();
    let canonical = world.type_registry().get(both_type_id).unwrap();
    assert_eq!(
        world
            .reflected_component_type_info(both_type_id)
            .unwrap()
            .rust_name,
        canonical.rust_name
    );
    assert_eq!(
        world
            .reflected_resource_type_info(both_type_id)
            .unwrap()
            .rust_name,
        canonical.rust_name
    );
}

#[test]
fn world_role_registration_reads_descriptor_once_and_reuses_registry_metadata() {
    DESCRIPTOR_PROBE_CALLS.store(0, Ordering::SeqCst);
    let mut world = World::new();
    world.register_reflected_component::<DescriptorProbe>();

    let type_id = TypeId::of::<DescriptorProbe>();
    assert_eq!(DESCRIPTOR_PROBE_CALLS.load(Ordering::SeqCst), 1);

    let canonical = world.type_registry().get(type_id).unwrap();
    let reflected = world.reflected_component_type_info(type_id).unwrap();
    assert_eq!(canonical.display_name, "first");
    assert_eq!(reflected.display_name, canonical.display_name);
    assert_eq!(world.reflected_component_types()[0].display_name, "first");
    assert_eq!(DESCRIPTOR_PROBE_CALLS.load(Ordering::SeqCst), 1);
}

#[test]
fn ordinary_component_registration_does_not_opt_into_reflection() {
    let mut world = World::new();
    let entity = world.spawn(ComponentA { value: 7 }).unwrap();

    assert!(world.has_component_type(TypeId::of::<ComponentA>()));
    assert!(!world.has_reflected_component_type(TypeId::of::<ComponentA>()));
    assert!(
        world
            .reflected_component_value_ref(entity, TypeId::of::<ComponentA>())
            .is_none()
    );
    assert_eq!(world.type_registry().types().count(), 0);
}

#[test]
fn generic_and_const_generic_descriptors_are_monomorphization_local() {
    let generic_u32 = Generic::<u32>::type_info();
    let generic_string = Generic::<String>::type_info();
    let const_four = ConstGeneric::<4>::type_info();
    let const_eight = ConstGeneric::<8>::type_info();

    assert_ne!(generic_u32.rust_name, generic_string.rust_name);
    assert_ne!(const_four.rust_name, const_eight.rust_name);
    assert_eq!(
        generic_u32
            .struct_info()
            .unwrap()
            .field_at(0)
            .unwrap()
            .type_info()
            .rust_name,
        "u32"
    );
    assert_eq!(
        generic_string
            .struct_info()
            .unwrap()
            .field_at(0)
            .unwrap()
            .type_info()
            .rust_name,
        "alloc::string::String"
    );
    assert_ne!(
        const_four
            .struct_info()
            .unwrap()
            .field_at(0)
            .unwrap()
            .type_info()
            .rust_name,
        const_eight
            .struct_info()
            .unwrap()
            .field_at(0)
            .unwrap()
            .type_info()
            .rust_name
    );
}

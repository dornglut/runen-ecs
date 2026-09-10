#![allow(non_camel_case_types)]

#[derive(runen_ecs::Component, runen_ecs::Resource)]
struct Counter;

#[derive(runen_ecs::Resource)]
struct Marker<const N: usize>;

#[derive(runen_ecs::SystemParam)]
struct WorldGroup<'w> {
    counter: runen_ecs::Res<'w, Counter>,
}

#[derive(runen_ecs::SystemParam)]
struct QueryGroup<'w, 's> {
    query: runen_ecs::Query<'w, 's, &'static Counter>,
}

#[derive(runen_ecs::SystemParam)]
struct NestedQueryGroup<'w, 's> {
    inner: QueryGroup<'w, 's>,
}

#[derive(runen_ecs::SystemParam)]
struct GenericConstGroup<'w, T: runen_ecs::Resource, const N: usize> {
    value: runen_ecs::Res<'w, T>,
    marker: runen_ecs::Res<'w, Marker<N>>,
}

#[derive(runen_ecs::SystemParam)]
struct GeneratedNameCollision<'w, world: runen_ecs::Resource> {
    value: runen_ecs::Res<'w, world>,
}

fn assert_param<P: runen_ecs::SystemParam>() {}

fn main() {
    assert_param::<WorldGroup<'static>>();
    assert_param::<QueryGroup<'static, 'static>>();
    assert_param::<NestedQueryGroup<'static, 'static>>();
    assert_param::<GenericConstGroup<'static, Counter, 3>>();
    assert_param::<GeneratedNameCollision<'static, Counter>>();
}

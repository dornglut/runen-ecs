#[derive(runen_ecs::Resource)]
struct Counter;

#[derive(runen_ecs::SystemParam)]
struct TooMany<'w, 's, 'a> {
    value: runen_ecs::Res<'w, Counter>,
    cached: runen_ecs::Query<'w, 's, &'static Counter>,
    other: runen_ecs::Res<'a, Counter>,
}

fn main() {}

#[derive(runen_ecs::SystemParam)]
struct WrongOrder<'w, 's> {
    query: runen_ecs::Query<'s, 'w, &'static Marker>,
}

#[derive(runen_ecs::Component)]
struct Marker;

fn main() {}

use criterion::{Criterion, criterion_group, criterion_main};
use runen_ecs::prelude::*;
use std::hint::black_box;

#[derive(Debug, Copy, Clone, Component)]
struct Position(u32);

#[derive(Debug, Copy, Clone, Component)]
struct Velocity(u32);

#[derive(Debug, Default, Resource)]
struct Count(usize);

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

fn build_world(count: usize) -> World {
    let mut world = World::new();
    for index in 0..count {
        world.spawn((Position(index as u32), Velocity(1))).unwrap();
    }
    world
}

fn bench_entity_component_insertion(c: &mut Criterion) {
    c.bench_function("entity_component_insertion_1000", |b| {
        b.iter(|| {
            let mut world = World::new();
            for index in 0..1000 {
                black_box(world.spawn((Position(index), Velocity(1))).unwrap());
            }
        });
    });
}

fn bench_query_iteration(c: &mut Criterion) {
    let world = build_world(10_000);
    let query = world.query_state::<(&Position, &Velocity), ()>();
    c.bench_function("query_iteration_10000", |b| {
        b.iter(|| {
            let checksum = query
                .iter(&world)
                .map(|(position, velocity)| position.0 + velocity.0)
                .sum::<u32>();
            black_box(checksum);
        });
    });
}

fn bench_archetype_transition(c: &mut Criterion) {
    c.bench_function("archetype_transition_1000", |b| {
        b.iter(|| {
            let mut world = build_world(1000);
            let entities = world
                .query_state::<(Entity, &Position), ()>()
                .iter(&world)
                .map(|(entity, _)| entity)
                .collect::<Vec<_>>();
            for entity in entities {
                world.insert(entity, Position(2)).unwrap();
            }
            black_box(world.query_state::<&Position, ()>().iter(&world).count());
        });
    });
}

fn increment(mut query: Query<&mut Position>) {
    for position in query.iter() {
        position.0 += 1;
    }
}

fn bench_serial_schedule_execution(c: &mut Criterion) {
    let mut world = build_world(10_000);
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, increment);
    c.bench_function("serial_schedule_execution_10000", |b| {
        b.iter(|| runtime.run_schedule::<Update>(&mut world).unwrap());
    });
}

fn queue_spawn(mut commands: Commands) {
    commands.spawn((Position(3), Velocity(4)));
}

fn count_entities(mut query: Query<&Position>, mut count: ResMut<Count>) {
    count.0 = query.iter().count();
}

fn bench_deferred_command_application(c: &mut Criterion) {
    let mut world = World::new();
    world.insert_resource(Count::default());
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (queue_spawn, count_entities));
    c.bench_function("deferred_command_application", |b| {
        b.iter(|| runtime.run_schedule::<Update>(&mut world).unwrap());
    });
}

criterion_group!(
    semantic_baseline,
    bench_entity_component_insertion,
    bench_query_iteration,
    bench_archetype_transition,
    bench_serial_schedule_execution,
    bench_deferred_command_application,
);
criterion_main!(semantic_baseline);

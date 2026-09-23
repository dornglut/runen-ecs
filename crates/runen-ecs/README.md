# RunenECS

RunenECS is a deterministic entity-component-system framework providing worlds,
components, resources, queries, deferred structural commands, and ECS-native
schedule semantics.

The implementation is maintained in this standalone repository. It was
transferred from the accepted Runenwerk C9 boundary at
`b7e3d55c76be0acebf3ce260e7ee282ea1787e29`; that provenance does not create a
second current implementation authority.

## Quick start

A normal RunenECS system is an ordinary Rust function whose parameters describe
its ECS access. Register it on a schedule, then run that schedule against a
`World`:

```rust
use runen_ecs::prelude::*;

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {}

#[derive(Component)]
struct Position(f32);

#[derive(Component)]
struct Velocity(f32);

#[derive(Resource)]
struct DeltaTime(f32);

fn integrate(mut bodies: Query<(&mut Position, &Velocity)>, dt: Res<DeltaTime>) {
    for (position, velocity) in bodies.iter() {
        position.0 += velocity.0 * dt.0;
    }
}

fn main() {
    let mut world = World::new();
    world.spawn((Position(0.0), Velocity(4.0))).unwrap();
    world.insert_resource(DeltaTime(0.5));

    let mut runtime = Runtime::new();
    runtime.add_systems(Update, integrate).unwrap();
    runtime.run_schedule::<Update>(&mut world).unwrap();
}
```

Ordinary system registration is the proven-transferable path. Use
`.on_invoker_thread()` only when a system genuinely requires the thread that
invokes its schedule. Transferable eligibility is not a promise that a system
currently runs on a worker or runs in parallel.

`Commands` is the normal deferred structural-mutation recorder and accepts only
transfer-safe deferred work. Code that genuinely needs arbitrary local /
`!Send` deferred work imports `LocalCommands` explicitly and pairs that system
with `.on_invoker_thread()`. `BatchCommands` and `LocalBatchCommands` follow
the same default-versus-explicit-local distinction.

Ordinary query iteration order is not a public semantic contract. Likewise,
physical executor grouping such as worker cohorts or stages is not part of the
schedule API: explicit ordering expresses semantic precedence.

Typed relations are a separate ECS domain rather than relationship components.
A relation type chooses directed or symmetric semantics, and callers access its
single authoritative edge set through `world.relations::<R>()` or
`world.relations_mut::<R>()`. Entity liveness and despawn remain owned by the
World; RunenGraph is private structural machinery and does not introduce a
second public identity or error model.

## Examples

Run examples from the repository root.

### Getting started

| Example | Purpose | Run |
| --- | --- | --- |
| `world_basics` | Create a World, spawn bundles, access entities/components, and use resources. | `cargo run -p runen-ecs --example world_basics` |
| `queries` | Query directly from a World with tuples, filters, optional components, `get`, and `single`. | `cargo run -p runen-ecs --example queries` |
| `systems` | Register and run ordinary systems using `Query`, `Res`, and `ResMut`. | `cargo run -p runen-ecs --example systems` |

### Core semantics

| Example | Purpose | Run |
| --- | --- | --- |
| `relations` | Model typed directed or symmetric relationships between live ECS entities without duplicating relationship components. | `cargo run -p runen-ecs --example relations` |
| `deferred_commands` | Stage transfer-safe structural changes and observe when they become published. | `cargo run -p runen-ecs --example deferred_commands` |
| `change_observation` | Observe `Added`, conservative `Changed`, and removed-component windows. | `cargo run -p runen-ecs --example change_observation` |
| `scheduling` | Express required and optional semantic precedence with system sets. | `cargo run -p runen-ecs --example scheduling` |
| `system_mobility` | Contrast normal transferable registration with an explicit invoker-thread-only system. | `cargo run -p runen-ecs --example system_mobility` |

### Integrated example

| Example | Purpose | Run |
| --- | --- | --- |
| `standalone_simulation` | Combine deferred spawning, ordered simulation, queries, and resources in a small coherent loop. | `cargo run -p runen-ecs --example standalone_simulation` |

The focused conformance tests remain the authority for edge cases and failure
semantics; examples are intentionally small teaching programs.

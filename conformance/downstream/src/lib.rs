use ecs_under_test::prelude::*;
use ecs_under_test::{Reflect, RuntimeError, SystemParam, WorldMut};

#[derive(Debug, Clone, Component, Reflect)]
pub struct Position {
    pub x: i32,
}

#[derive(Debug, Clone, Component, Reflect)]
pub struct Velocity {
    pub x: i32,
}

#[derive(Debug, Default, Resource, Reflect)]
pub struct Frame {
    pub value: u32,
}

#[derive(Debug, Bundle)]
pub struct ActorBundle {
    pub position: Position,
    pub velocity: Velocity,
}

#[derive(SystemParam)]
pub struct SimulationParams<'w, 's> {
    pub positions: Query<'w, 's, &'static mut Position>,
    pub frame: ResMut<'w, Frame>,
}

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

fn advance(mut params: SimulationParams<'_, '_>) {
    for position in params.positions.iter() {
        position.x += 1;
    }
    params.frame.value += 1;
}

fn touch_world(mut world: WorldMut<'_>) {
    world.resource_mut::<Frame>().unwrap().value += 1;
}

pub fn run_conformance() -> Result<(), RuntimeError> {
    let mut world = World::new();
    world.insert_resource(Frame::default());
    world
        .spawn(ActorBundle {
            position: Position { x: 0 },
            velocity: Velocity { x: 1 },
        })
        .unwrap();

    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, (advance, touch_world));
    runtime.run_schedule::<Update>(&mut world)?;

    assert_eq!(world.resource::<Frame>().unwrap().value, 2);
    assert_eq!(
        world
            .query_state::<&Position, ()>()
            .single(&world)
            .unwrap()
            .x,
        1
    );
    Ok(())
}

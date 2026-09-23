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

pub struct Follows;

impl Relation for Follows {
    type Kind = Directed;
    const SELF: SelfRelation = SelfRelation::Allow;
}

#[derive(Copy, Clone)]
struct Update;

impl ScheduleLabel for Update {
    fn name() -> &'static str {
        "Update"
    }
}

#[derive(Copy, Clone, SystemSet)]
enum Phase {
    Prepare,
    Simulate,
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
    let prepare = Phase::Prepare.key();
    let simulate = Phase::Simulate.key();
    assert_eq!(prepare.name(), "Phase::Prepare");
    assert_eq!(simulate.name(), "Phase::Simulate");
    assert_ne!(prepare, simulate);
    assert_eq!(prepare, Phase::Prepare.key());

    let mut world = World::new();
    world.insert_resource(Frame::default());
    let actor = world
        .spawn(ActorBundle {
            position: Position { x: 0 },
            velocity: Velocity { x: 1 },
        })
        .unwrap();
    assert!(world
        .relations_mut::<Follows>()
        .insert(actor, actor)
        .unwrap());
    assert_eq!(
        world
            .relations::<Follows>()
            .targets(actor)
            .unwrap()
            .iter()
            .collect::<Vec<_>>(),
        vec![actor]
    );

    let mut runtime = Runtime::new();
    runtime
        .add_systems(Update, (advance, touch_world.on_invoker_thread()))
        .unwrap();
    runtime.run_schedule::<Update>(&mut world)?;

    assert_eq!(world.resource::<Frame>().unwrap().value, 2);
    assert_eq!(world.query::<&Position>().single(&world).unwrap().x, 1);
    Ok(())
}

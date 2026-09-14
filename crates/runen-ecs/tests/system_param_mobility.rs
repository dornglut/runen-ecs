use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::MutexGuard;

use runen_ecs::{
    Component, Entity, Query, QueryState, RemovedQuery, RemovedState, Res, ResMut, Resource,
    Runtime, TransferableSystemParam, World,
};

#[allow(dead_code)]
#[derive(Component)]
struct SendButNotSync(Cell<u32>);

#[derive(Component)]
#[allow(dead_code)]
struct SyncButNotSend(MutexGuard<'static, ()>);

#[allow(dead_code)]
#[derive(Component)]
struct ThreadBound(Rc<()>);

#[allow(dead_code)]
#[derive(Resource)]
struct SendResource(Cell<u32>);

#[derive(Resource)]
#[allow(dead_code)]
struct SyncResource(MutexGuard<'static, ()>);

#[derive(runen_ecs::SystemParam)]
struct TransferableGroup<'w, 's> {
    _shared: Query<'w, 's, &'static SyncButNotSend>,
    _mutable: Query<'w, 's, &'static mut SendButNotSync>,
    _filtered: Query<'w, 's, &'static SyncButNotSend, runen_ecs::With<ThreadBound>>,
    _removed: RemovedQuery<'w, 's, ThreadBound>,
}

#[derive(runen_ecs::SystemParam)]
struct NestedTransferableGroup<'w, 's> {
    _inner: TransferableGroup<'w, 's>,
    _resource: Res<'w, SyncResource>,
}

fn assert_transferable<P: TransferableSystemParam>()
where
    P::State: Send,
{
}

fn assert_send<T: Send>() {}

#[test]
fn representative_parameter_shapes_are_transferable() {
    assert_transferable::<Res<'static, SyncResource>>();
    assert_transferable::<ResMut<'static, SendResource>>();

    assert_transferable::<Query<'static, 'static, &SyncButNotSend>>();
    assert_transferable::<Query<'static, 'static, &mut SendButNotSync>>();
    assert_transferable::<Query<'static, 'static, (Entity, &SyncButNotSend)>>();
    assert_transferable::<Query<'static, 'static, (Entity, &mut SendButNotSync)>>();
    assert_transferable::<Query<'static, 'static, (&mut SendButNotSync, &SyncButNotSend)>>();
    assert_transferable::<Query<'static, 'static, (&SyncButNotSend, &mut SendButNotSync)>>();
    assert_transferable::<Query<'static, 'static, Option<&SyncButNotSend>>>();
    assert_transferable::<Query<'static, 'static, Option<&mut SendButNotSync>>>();
    assert_transferable::<Query<'static, 'static, (&mut SendButNotSync, Option<&SyncButNotSend>)>>(
    );
    assert_transferable::<Query<'static, 'static, (&SyncButNotSend, Option<&mut SendButNotSync>)>>(
    );
    assert_transferable::<
        Query<'static, 'static, (&mut SendButNotSync, Option<&mut SendButNotSync>)>,
    >();
    assert_transferable::<Query<'static, 'static, (Entity, Option<&SyncButNotSend>)>>();
    assert_transferable::<
        Query<'static, 'static, (&SyncButNotSend, &SyncButNotSend, &SyncButNotSend)>,
    >();
    assert_transferable::<
        Query<'static, 'static, (&mut SendButNotSync, &SyncButNotSend, &SyncButNotSend)>,
    >();
    assert_transferable::<
        Query<'static, 'static, (&mut SendButNotSync, &mut SendButNotSync, &SyncButNotSend)>,
    >();

    assert_transferable::<Query<'static, 'static, &SyncButNotSend, runen_ecs::With<ThreadBound>>>();
    assert_transferable::<Query<'static, 'static, &SyncButNotSend, runen_ecs::Without<ThreadBound>>>(
    );
    assert_transferable::<Query<'static, 'static, &SyncButNotSend, runen_ecs::Added<ThreadBound>>>(
    );
    assert_transferable::<Query<'static, 'static, &SyncButNotSend, runen_ecs::Changed<ThreadBound>>>(
    );
    assert_transferable::<RemovedQuery<'static, 'static, ThreadBound>>();

    assert_transferable::<(
        Res<'static, SyncResource>,
        ResMut<'static, SendResource>,
        Query<'static, 'static, &SyncButNotSend>,
    )>();
    assert_transferable::<TransferableGroup<'static, 'static>>();
    assert_transferable::<NestedTransferableGroup<'static, 'static>>();
}

#[test]
fn cached_state_is_send_without_being_required_to_be_sync() {
    assert_send::<QueryState<&SendButNotSync, runen_ecs::With<ThreadBound>>>();
    assert_send::<QueryState<&SyncButNotSend, runen_ecs::Changed<ThreadBound>>>();
    assert_send::<RemovedState<ThreadBound>>();
}

#[test]
fn metadata_change_filters_keep_scheduler_component_read_access() {
    let world = World::new();
    let changed = world.query_state::<&SyncButNotSend, runen_ecs::Changed<ThreadBound>>();
    assert!(
        changed
            .access()
            .component_reads()
            .iter()
            .any(|access| access.type_id() == TypeId::of::<ThreadBound>())
    );
}

#[test]
fn query_state_scratch_can_be_reused_after_early_drop_and_with_live_iterators() {
    let mut world = World::new();
    let guard = Box::leak(Box::new(std::sync::Mutex::new(())))
        .lock()
        .unwrap();
    world.spawn(SyncButNotSend(guard)).unwrap();
    let guard = Box::leak(Box::new(std::sync::Mutex::new(())))
        .lock()
        .unwrap();
    world.spawn(SyncButNotSend(guard)).unwrap();

    let query = world.query_state::<&SyncButNotSend, ()>();
    let mut first = query.iter(&world);
    let mut second = query.iter(&world);
    assert!(first.next().is_some());
    assert!(second.next().is_some());
    drop(first);
    drop(second);

    let reused = query.iter(&world);
    assert_eq!(reused.count(), 2);
}

#[test]
fn ordinary_serial_registration_remains_permissive() {
    #[derive(Copy, Clone)]
    struct Update;

    impl runen_ecs::ScheduleLabel for Update {
        fn name() -> &'static str {
            "MobilityUpdate"
        }
    }

    let mut world = World::new();
    let ran = Rc::new(RefCell::new(false));
    let captured = Rc::clone(&ran);
    let mut runtime = Runtime::new();
    runtime.add_systems::<Update, _, _>(&mut world, move || {
        *captured.borrow_mut() = true;
    });
    runtime.run_schedule::<Update>(&mut world).unwrap();
    assert!(*ran.borrow());
}

#[test]
fn mobility_negatives_are_rejected_by_trybuild() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/mobility_shared_missing_sync.rs");
    cases.compile_fail("tests/ui/mobility_mut_missing_send.rs");
    cases.compile_fail("tests/ui/mobility_world_mut.rs");
    cases.compile_fail("tests/ui/mobility_commands.rs");
    cases.compile_fail("tests/ui/mobility_derive_local.rs");
    cases.compile_fail("tests/ui/mobility_safe_forgery.rs");
}

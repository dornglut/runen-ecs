use super::World;
use super::change_tracking::{
    ChangeCursor, panic_change_cursor_exhausted, panic_parallel_executor_violation,
};
use crate::entity::Entity;
use std::any::TypeId;
use std::ptr::NonNull;
#[cfg(target_has_atomic = "ptr")]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Copy, Clone)]
pub(crate) struct PrevalidatedComponentMutationTarget {
    changed_tick: NonNull<ChangeCursor>,
}

impl PrevalidatedComponentMutationTarget {
    pub(crate) fn new(changed_tick: NonNull<ChangeCursor>) -> Self {
        Self { changed_tick }
    }

    pub(crate) fn changed_tick(self) -> NonNull<ChangeCursor> {
        self.changed_tick
    }
}

// Safety: targets are created only from row metadata preflighted under either
// the serial World borrow or ParallelWorldLease structural freeze. Workers only
// carry the address; only invoker-side journal reconciliation writes through it
// after workers have joined and before structural relocation becomes possible.
unsafe impl Send for PrevalidatedComponentMutationTarget {}

enum MutationEvent {
    ComponentModified {
        entity: Entity,
        component_type: TypeId,
        target: Option<PrevalidatedComponentMutationTarget>,
    },
    ResourceModified {
        resource_type: TypeId,
    },
}

// A provisional grant is NOT an admitted mutation event. Each worker's
// credits remain globally redeemable, including while its owner is paused,
// panicked or finished. No user code executes while the distribution lock is held.
const WORKER_CREDIT_BATCH: usize = 256;

#[repr(align(128))]
struct WorkerCreditEscrow {
    #[cfg(target_has_atomic = "ptr")]
    unspent: AtomicUsize,
    #[cfg(not(target_has_atomic = "ptr"))]
    unspent: Mutex<usize>,
}

impl WorkerCreditEscrow {
    fn new() -> Self {
        Self {
            #[cfg(target_has_atomic = "ptr")]
            unspent: AtomicUsize::new(0),
            #[cfg(not(target_has_atomic = "ptr"))]
            unspent: Mutex::new(0),
        }
    }

    fn try_claim(&self) -> bool {
        #[cfg(target_has_atomic = "ptr")]
        {
            let mut observed = self.unspent.load(Ordering::Relaxed);
            while observed != 0 {
                match self.unspent.compare_exchange_weak(
                    observed,
                    observed - 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return true,
                    Err(actual) => observed = actual,
                }
            }
            false
        }

        #[cfg(not(target_has_atomic = "ptr"))]
        {
            let mut balance = self.unspent.lock().unwrap_or_else(|_| {
                panic_parallel_executor_violation("worker escrow lock was poisoned")
            });
            if *balance == 0 {
                false
            } else {
                *balance -= 1;
                true
            }
        }
    }

    fn balance(&self) -> usize {
        #[cfg(target_has_atomic = "ptr")]
        {
            self.unspent.load(Ordering::Relaxed)
        }
        #[cfg(not(target_has_atomic = "ptr"))]
        {
            *self.unspent.lock().unwrap_or_else(|_| {
                panic_parallel_executor_violation("worker escrow lock was poisoned")
            })
        }
    }

    // Invoked only under the distribution mutex after checking this slot is
    // empty. Other threads may claim from it but cannot deposit into it.
    fn deposit(&self, grant: usize) {
        if self.balance() != 0 {
            panic_parallel_executor_violation("worker escrow refilled while nonempty");
        }
        #[cfg(target_has_atomic = "ptr")]
        self.unspent.store(grant, Ordering::Relaxed);
        #[cfg(not(target_has_atomic = "ptr"))]
        {
            *self.unspent.lock().unwrap_or_else(|_| {
                panic_parallel_executor_violation("worker escrow lock was poisoned")
            }) = grant;
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConcurrentMutationCapacity {
    base_cursor: ChangeCursor,
    state: Arc<ConcurrentMutationCapacityState>,
    worker_slot: usize,
}

struct ConcurrentMutationCapacityState {
    remaining: u128,
    unassigned: Mutex<u128>,
    escrows: Box<[WorkerCreditEscrow]>,
}

impl ConcurrentMutationCapacityState {
    fn try_reserve(&self, worker_slot: usize) -> bool {
        let own = self.escrows.get(worker_slot).unwrap_or_else(|| {
            panic_parallel_executor_violation("mutation capacity worker slot is missing")
        });

        loop {
            if own.try_claim() {
                return true;
            }

            // Refills and the terminal zero scan use one short-lived lock.
            // Worker-local claims never require this lock. In particular,
            // exhausted peers never wait for paused user code to return credit.
            let mut unassigned = self.unassigned.lock().unwrap_or_else(|_| {
                panic_parallel_executor_violation("mutation distribution lock was poisoned")
            });

            // Another claimant may have refilled our slot while we waited.
            if own.try_claim() {
                return true;
            }

            if *unassigned != 0 {
                let grant = (*unassigned).min(WORKER_CREDIT_BATCH as u128) as usize;
                *unassigned -= grant as u128;
                own.deposit(grant);
                continue;
            }

            // With this mutex held and U == 0, no slot balance can increase.
            // CAS claims may decrease it. A complete zero scan therefore
            // proves exact exhaustion, not merely fully leased capacity.
            for donor in &self.escrows {
                if donor.try_claim() {
                    return true;
                }
            }
            return false;
        }
    }

    #[cfg(test)]
    fn total_admitted_after_join(&self) -> u128 {
        let unassigned = *self.unassigned.lock().unwrap_or_else(|_| {
            panic_parallel_executor_violation("mutation distribution lock was poisoned")
        });
        let outstanding = self
            .escrows
            .iter()
            .map(|escrow| escrow.balance() as u128)
            .sum::<u128>();
        self.remaining
            .checked_sub(unassigned + outstanding)
            .unwrap_or_else(|| {
                panic_parallel_executor_violation("mutation capacity accounting overflow")
            })
    }
}

impl ConcurrentMutationCapacity {
    pub(crate) fn new(base_cursor: ChangeCursor, workers: usize) -> Self {
        if workers == 0 {
            panic_parallel_executor_violation("mutation capacity requires a worker");
        }
        let ordinal = ((base_cursor.epoch() as u128) << 64) | base_cursor.tick() as u128;
        let remaining = u128::MAX - ordinal;
        Self {
            base_cursor,
            state: Arc::new(ConcurrentMutationCapacityState {
                remaining,
                unassigned: Mutex::new(remaining),
                escrows: (0..workers)
                    .map(|_| WorkerCreditEscrow::new())
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            }),
            worker_slot: 0,
        }
    }

    pub(crate) fn for_worker(&self, worker_slot: usize) -> Self {
        if worker_slot >= self.state.escrows.len() {
            panic_parallel_executor_violation("mutation capacity worker slot is missing");
        }
        Self {
            base_cursor: self.base_cursor,
            state: Arc::clone(&self.state),
            worker_slot,
        }
    }

    fn reserve_next_event(&self) {
        if !self.state.try_reserve(self.worker_slot) {
            panic_change_cursor_exhausted();
        }
    }

    #[cfg(test)]
    fn admitted_for_test(&self) -> u128 {
        self.state.total_admitted_after_join()
    }
}

enum MutationReservation {
    Serial {
        next_reserved_cursor: Option<ChangeCursor>,
    },
    Concurrent {
        capacity: ConcurrentMutationCapacity,
    },
}

/// Ordered, invocation-local mutation-observation events.
pub(crate) struct MutationJournal {
    base_cursor: ChangeCursor,
    reservation: MutationReservation,
    events: Vec<MutationEvent>,
}

impl MutationJournal {
    pub(crate) fn new(world: &World) -> Self {
        let base_cursor = world.current_change_cursor();
        Self {
            base_cursor,
            reservation: MutationReservation::Serial {
                next_reserved_cursor: base_cursor.next(),
            },
            events: Vec::new(),
        }
    }

    pub(crate) fn new_concurrent(
        base_cursor: ChangeCursor,
        capacity: ConcurrentMutationCapacity,
    ) -> Self {
        assert_eq!(
            capacity.base_cursor, base_cursor,
            "worker journal capacity must match the cohort base cursor"
        );
        Self {
            base_cursor,
            reservation: MutationReservation::Concurrent { capacity },
            events: Vec::new(),
        }
    }

    pub(crate) fn record_component_modified(&mut self, entity: Entity, component_type: TypeId) {
        self.record_component_modified_inner(entity, component_type, None);
    }

    pub(crate) fn record_prevalidated_component_modified(
        &mut self,
        entity: Entity,
        component_type: TypeId,
        target: PrevalidatedComponentMutationTarget,
    ) {
        self.record_component_modified_inner(entity, component_type, Some(target));
    }

    fn record_component_modified_inner(
        &mut self,
        entity: Entity,
        component_type: TypeId,
        target: Option<PrevalidatedComponentMutationTarget>,
    ) {
        self.reserve_next_event();
        self.events.push(MutationEvent::ComponentModified {
            entity,
            component_type,
            target,
        });
    }

    pub(crate) fn record_resource_modified(&mut self, resource_type: TypeId) {
        self.reserve_next_event();
        self.events
            .push(MutationEvent::ResourceModified { resource_type });
    }

    fn reserve_next_event(&mut self) {
        match &mut self.reservation {
            MutationReservation::Serial {
                next_reserved_cursor,
            } => {
                let reserved = next_reserved_cursor
                    .take()
                    .unwrap_or_else(|| panic_change_cursor_exhausted());
                *next_reserved_cursor = reserved.next();
            }
            MutationReservation::Concurrent { capacity } => capacity.reserve_next_event(),
        }
    }

    pub(crate) fn concurrent_base_cursor(&self) -> Option<ChangeCursor> {
        matches!(self.reservation, MutationReservation::Concurrent { .. })
            .then_some(self.base_cursor)
    }

    pub(crate) fn commit(self, world: &mut World) {
        if self.events.is_empty() {
            return;
        }
        assert!(
            matches!(self.reservation, MutationReservation::Serial { .. }),
            "serial journal commit cannot reconcile a worker journal"
        );
        assert_eq!(
            world.current_change_cursor(),
            self.base_cursor,
            "mutation journal base cursor changed before reconciliation"
        );
        self.replay(world);
    }

    pub(crate) fn commit_concurrent(self, world: &mut World) {
        assert!(
            matches!(self.reservation, MutationReservation::Concurrent { .. }),
            "worker journal reconciliation requires concurrent capacity admission"
        );
        assert!(
            self.base_cursor.is_from(world.scope_id()),
            "worker journal belongs to a different World lineage"
        );
        self.replay(world);
    }

    fn replay(self, world: &mut World) {
        // The invoker owns the unique World and all workers have joined.
        // Type-level publication can be deferred only until the LAST event
        // of a contiguous prevalidated same-type run: the next event is the
        // publication barrier and no callback observes intermediate state.
        // Every event still advances a checked cursor and writes its row tick.
        let mut events = self.events.into_iter().peekable();
        while let Some(event) = events.next() {
            match event {
                MutationEvent::ComponentModified {
                    entity,
                    component_type,
                    target,
                } => match target {
                    Some(target) => {
                        let publish_type_change = !matches!(
                            events.peek(),
                            Some(MutationEvent::ComponentModified {
                                component_type: next_type,
                                target: Some(_),
                                ..
                            }) if *next_type == component_type
                        );
                        world.commit_prevalidated_component_mutation_event(
                            entity,
                            component_type,
                            target.changed_tick(),
                            publish_type_change,
                        );
                    }
                    None => world.commit_component_mutation_event(entity, component_type),
                },
                MutationEvent::ResourceModified { resource_type } => {
                    world.commit_resource_mutation_event(resource_type)
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "mutation_diagnostic.rs"]
mod mutation_diagnostic;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::change_tracking::{FrameworkInvariantKind, framework_invariant_kind};
    use crate::{Component, Resource};
    use std::cell::Cell;

    #[derive(Component)]
    struct A;

    #[derive(Component)]
    struct B;

    #[test]
    fn cursor_exhaustion_has_private_identity() {
        let world = World::new();
        let scope = world.scope_id();
        let mut cursor = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX);
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::super::change_tracking::advance_change_cursor(&mut cursor);
        }))
        .expect_err("the invariant payload must panic");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );

        let resumed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            std::panic::resume_unwind(payload);
        }))
        .expect_err("the resumed invariant payload must panic");
        assert_eq!(
            framework_invariant_kind(resumed.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );

        let user_payload = std::panic::catch_unwind(|| {
            panic!("ECS change cursor exhausted");
        })
        .expect_err("the user payload must panic");
        assert_eq!(framework_invariant_kind(user_payload.as_ref()), None);
    }

    #[test]
    fn concurrent_reservation_order_does_not_choose_public_cursor_order() {
        let mut world = World::new();
        let a = world.spawn(A).unwrap();
        let b = world.spawn(B).unwrap();
        let base = world.current_change_cursor();
        let capacity = ConcurrentMutationCapacity::new(base, 1);

        let mut second = MutationJournal::new_concurrent(base, capacity.clone());
        second.record_component_modified(b, TypeId::of::<B>());
        let mut first = MutationJournal::new_concurrent(base, capacity);
        first.record_component_modified(a, TypeId::of::<A>());

        first.commit_concurrent(&mut world);
        let a_tick = world.archetype_component_metadata::<A>(a).unwrap().1;
        second.commit_concurrent(&mut world);
        let b_tick = world.archetype_component_metadata::<B>(b).unwrap().1;

        assert!(a_tick < b_tick);
        assert_eq!(world.current_change_cursor().tick(), base.tick() + 2);
    }

    #[test]
    fn prevalidated_component_target_publishes_only_during_reconciliation() {
        let mut world = World::new();
        let entity = world.spawn(A).unwrap();
        let before = world.archetype_component_metadata::<A>(entity).unwrap().1;
        let component_type = TypeId::of::<A>();
        let spans = world
            .archetype_registry
            .collect_journal_query_spans(
                &[component_type],
                &[],
                &[component_type],
                &[component_type],
            )
            .unwrap();
        let changed_tick = spans[0].changed_tick_ptr_at(0, 0).unwrap();

        let mut journal = MutationJournal::new(&world);
        journal.record_prevalidated_component_modified(
            entity,
            component_type,
            PrevalidatedComponentMutationTarget::new(changed_tick),
        );

        assert_eq!(
            world.archetype_component_metadata::<A>(entity).unwrap().1,
            before,
            "recording a journal target must not publish row change metadata early"
        );

        journal.commit(&mut world);

        assert!(
            world.archetype_component_metadata::<A>(entity).unwrap().1 > before,
            "reconciliation must publish the canonical row change cursor"
        );
    }

    #[test]
    fn repeated_prevalidated_component_events_preserve_change_positions() {
        let mut world = World::new();
        let entity = world.spawn(A).unwrap();
        let base = world.current_change_cursor();
        let component_type = TypeId::of::<A>();
        let spans = world
            .archetype_registry
            .collect_journal_query_spans(
                &[component_type],
                &[],
                &[component_type],
                &[component_type],
            )
            .unwrap();
        let changed_tick = spans[0].changed_tick_ptr_at(0, 0).unwrap();

        let mut journal = MutationJournal::new(&world);
        let target = PrevalidatedComponentMutationTarget::new(changed_tick);
        journal.record_prevalidated_component_modified(entity, component_type, target);
        journal.record_prevalidated_component_modified(entity, component_type, target);
        journal.commit(&mut world);

        assert_eq!(world.current_change_cursor().tick(), base.tick() + 2);
        assert_eq!(
            world.archetype_component_metadata::<A>(entity).unwrap().1,
            world.current_change_cursor(),
            "the row must retain the last canonical repeated-mutation position"
        );
    }

    #[test]
    fn concurrent_capacity_rejects_unreserved_event_without_wrap() {
        let mut world = World::new();
        let a = world.spawn(A).unwrap();
        let b = world.spawn(B).unwrap();
        let scope = world.scope_id();
        let base = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1);
        world.set_change_cursor_for_test(base);
        let capacity = ConcurrentMutationCapacity::new(base, 1);

        let mut first = MutationJournal::new_concurrent(base, capacity.clone());
        first.record_component_modified(a, TypeId::of::<A>());
        let mut second = MutationJournal::new_concurrent(base, capacity);
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            second.record_component_modified(b, TypeId::of::<B>());
        }))
        .expect_err("the second reservation must exhaust absolute capacity");

        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
        first.commit_concurrent(&mut world);
        assert_eq!(
            world.current_change_cursor(),
            ChangeCursor::from_parts(scope, u64::MAX, u64::MAX)
        );
        assert!(world.component_changed_since::<A>(base).unwrap());
        assert!(!world.component_changed_since::<B>(base).unwrap());
    }
    #[test]
    fn portable_admission_rejects_at_terminal_cursor_without_wrap() {
        let world = World::new();
        let scope = world.scope_id();
        for (tick, allowed) in [(u64::MAX, 0), (u64::MAX - 1, 1), (u64::MAX - 2, 2)] {
            let base = ChangeCursor::from_parts(scope, u64::MAX, tick);
            let capacity = ConcurrentMutationCapacity::new(base, 1);
            for _ in 0..allowed {
                capacity.reserve_next_event();
            }
            assert_eq!(capacity.admitted_for_test(), allowed as u128);
            let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                capacity.reserve_next_event();
            }))
            .expect_err("reservation must fail at the absolute cursor limit");
            assert_eq!(
                framework_invariant_kind(payload.as_ref()),
                Some(FrameworkInvariantKind::ChangeCursorExhausted)
            );
            assert_eq!(capacity.admitted_for_test(), allowed as u128);
        }
    }

    #[test]
    fn concurrent_last_position_admits_one_worker_and_preserves_invariant_identity() {
        let world = World::new();
        let base = ChangeCursor::from_parts(world.scope_id(), u64::MAX, u64::MAX - 1);
        let capacity = ConcurrentMutationCapacity::new(base, 1);
        let barrier = Arc::new(std::sync::Barrier::new(4));
        let results = std::thread::scope(|scope| {
            let handles = (0..4)
                .map(|_| {
                    let capacity = capacity.clone();
                    let barrier = barrier.clone();
                    scope.spawn(move || {
                        barrier.wait();
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            capacity.reserve_next_event();
                        }))
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("admission probe worker escaped"))
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        for payload in results.into_iter().filter_map(Result::err) {
            assert_eq!(
                framework_invariant_kind(payload.as_ref()),
                Some(FrameworkInvariantKind::ChangeCursorExhausted)
            );
        }
        assert_eq!(capacity.admitted_for_test(), 1);
    }

    #[test]
    fn near_terminal_capacity_redeems_idle_worker_credits() {
        let world = World::new();
        let base = ChangeCursor::from_parts(world.scope_id(), u64::MAX, u64::MAX - 2);
        let capacity = ConcurrentMutationCapacity::new(base, 2);
        let first = capacity.for_worker(0);
        let second = capacity.for_worker(1);

        // A's first refill provisions both remaining positions, but admits
        // only one. B must be able to use A's unused credit without its help.
        first.reserve_next_event();
        assert_eq!(capacity.state.escrows[0].balance(), 1);
        assert_eq!(capacity.admitted_for_test(), 1);
        second.reserve_next_event();
        assert_eq!(capacity.admitted_for_test(), 2);

        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            first.reserve_next_event();
        }))
        .expect_err("exact exhaustion must reject a third event");
        assert_eq!(
            framework_invariant_kind(payload.as_ref()),
            Some(FrameworkInvariantKind::ChangeCursorExhausted)
        );
    }

    #[test]
    fn concurrent_owner_and_borrower_can_claim_last_credit_only_once() {
        let world = World::new();
        let base = ChangeCursor::from_parts(world.scope_id(), u64::MAX, u64::MAX - 1);
        let capacity = ConcurrentMutationCapacity::new(base, 2);
        // Seed one *unspent* credit, not an admitted event. Both actual
        // requesters contend for the same exact last position.
        *capacity.state.unassigned.lock().unwrap() = 0;
        capacity.state.escrows[0].deposit(1);
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let results = std::thread::scope(|scope| {
            let handles = (0..2)
                .map(|worker| {
                    let capacity = capacity.for_worker(worker);
                    let barrier = Arc::clone(&barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            capacity.reserve_next_event();
                        }))
                    })
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("admission test worker escaped"))
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        for payload in results.into_iter().filter_map(Result::err) {
            assert_eq!(
                framework_invariant_kind(payload.as_ref()),
                Some(FrameworkInvariantKind::ChangeCursorExhausted)
            );
        }
        assert_eq!(capacity.admitted_for_test(), 1);
    }

    #[test]
    fn provisioned_large_u128_tail_has_no_false_exhaustion() {
        let world = World::new();
        let scope = world.scope_id();

        for (base, remaining) in [
            (
                ChangeCursor::from_parts(scope, u64::MAX - 1, u64::MAX),
                u64::MAX as u128 + 1,
            ),
            (
                ChangeCursor::from_parts(scope, u64::MAX, 0),
                u64::MAX as u128,
            ),
        ] {
            let capacity = ConcurrentMutationCapacity::new(base, 1);
            assert_eq!(capacity.state.remaining, remaining);
            // Artificially account for an already-admitted prefix without
            // constructing billions of journal events or World positions.
            *capacity.state.unassigned.lock().unwrap() = 1;
            capacity.reserve_next_event();
            assert_eq!(capacity.admitted_for_test(), remaining);
            let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                capacity.reserve_next_event();
            }))
            .expect_err("no position may be admitted beyond the u128 tail");
            assert_eq!(
                framework_invariant_kind(payload.as_ref()),
                Some(FrameworkInvariantKind::ChangeCursorExhausted)
            );
        }
    }

    #[derive(Component)]
    struct Indexed(Cell<u64>);

    #[derive(Resource)]
    struct R;

    #[test]
    fn prevalidated_same_type_run_preserves_each_row_and_rebuilds_indexes() {
        let mut world = World::new();
        let first = world.spawn(Indexed(Cell::new(1))).unwrap();
        let second = world.spawn(Indexed(Cell::new(2))).unwrap();
        world.ensure_component_index::<Indexed, u64>(|value| value.0.get());
        assert_eq!(world.find_entity_by_index::<Indexed, u64>(&1), Some(first));
        assert_eq!(world.find_entity_by_index::<Indexed, u64>(&2), Some(second));
        let base = world.current_change_cursor();
        let type_id = TypeId::of::<Indexed>();
        let spans = world
            .archetype_registry
            .collect_journal_query_spans(&[type_id], &[], &[type_id], &[type_id])
            .unwrap();
        let a =
            PrevalidatedComponentMutationTarget::new(spans[0].changed_tick_ptr_at(0, 0).unwrap());
        let b =
            PrevalidatedComponentMutationTarget::new(spans[0].changed_tick_ptr_at(0, 1).unwrap());

        // Model a deferred worker write without directly publishing change
        // tracking: Cell is safe interior mutability in this *serial* test.
        world.get::<Indexed>(first).unwrap().0.set(9);
        let mut journal = MutationJournal::new(&world);
        journal.record_prevalidated_component_modified(first, type_id, a);
        journal.record_prevalidated_component_modified(second, type_id, b);
        journal.record_prevalidated_component_modified(first, type_id, a);
        journal.commit(&mut world);

        assert_eq!(world.current_change_cursor().tick(), base.tick() + 3);
        let middle = ChangeCursor::from_parts(world.scope_id(), base.epoch(), base.tick() + 2);
        assert_eq!(
            world
                .archetype_component_metadata::<Indexed>(second)
                .unwrap()
                .1,
            middle,
        );
        assert_eq!(
            world
                .archetype_component_metadata::<Indexed>(first)
                .unwrap()
                .1,
            world.current_change_cursor(),
        );
        assert!(world.component_changed_since::<Indexed>(middle).unwrap());
        assert!(
            !world
                .component_changed_since::<Indexed>(world.current_change_cursor())
                .unwrap()
        );
        assert_eq!(world.find_entity_by_index::<Indexed, u64>(&1), None);
        assert_eq!(world.find_entity_by_index::<Indexed, u64>(&9), Some(first));
        assert_eq!(world.find_entity_by_index::<Indexed, u64>(&2), Some(second));
    }

    #[test]
    fn mixed_prevalidated_fallback_and_resource_events_keep_reference_positions() {
        let mut world = World::new();
        let entity = world.spawn((A, B)).unwrap();
        world.insert_resource(R);
        let a_type = TypeId::of::<A>();
        let b_type = TypeId::of::<B>();
        let spans = world
            .archetype_registry
            .collect_journal_query_spans(
                &[a_type, b_type],
                &[],
                &[a_type, b_type],
                &[a_type, b_type],
            )
            .unwrap();
        let a_target =
            PrevalidatedComponentMutationTarget::new(spans[0].changed_tick_ptr_at(0, 0).unwrap());
        let b_target =
            PrevalidatedComponentMutationTarget::new(spans[0].changed_tick_ptr_at(1, 0).unwrap());
        let base = world.current_change_cursor();
        let mut journal = MutationJournal::new(&world);
        journal.record_prevalidated_component_modified(entity, a_type, a_target);
        journal.record_prevalidated_component_modified(entity, a_type, a_target);
        journal.record_prevalidated_component_modified(entity, b_type, b_target);
        journal.record_component_modified(entity, b_type);
        journal.record_prevalidated_component_modified(entity, a_type, a_target);
        journal.record_resource_modified(TypeId::of::<R>());
        journal.record_prevalidated_component_modified(entity, a_type, a_target);
        journal.commit(&mut world);

        assert_eq!(world.current_change_cursor().tick(), base.tick() + 7);
        let b_last = ChangeCursor::from_parts(world.scope_id(), base.epoch(), base.tick() + 4);
        let r_last = ChangeCursor::from_parts(world.scope_id(), base.epoch(), base.tick() + 6);
        assert_eq!(
            world.archetype_component_metadata::<B>(entity).unwrap().1,
            b_last,
        );
        assert_eq!(
            world.archetype_component_metadata::<A>(entity).unwrap().1,
            world.current_change_cursor(),
        );
        assert!(!world.component_changed_since::<B>(b_last).unwrap());
        assert!(world.component_changed_since::<A>(b_last).unwrap());
        assert!(!world.resource_changed_since::<R>(r_last).unwrap());
        assert!(world.resource_changed_since::<R>(b_last).unwrap());
    }

    #[test]
    fn prevalidated_run_preserves_cursor_epoch_rollover() {
        let mut world = World::new();
        let entity = world.spawn(A).unwrap();
        let type_id = TypeId::of::<A>();
        let spans = world
            .archetype_registry
            .collect_journal_query_spans(&[type_id], &[], &[type_id], &[type_id])
            .unwrap();
        let target =
            PrevalidatedComponentMutationTarget::new(spans[0].changed_tick_ptr_at(0, 0).unwrap());
        let base = ChangeCursor::from_parts(world.scope_id(), 4, u64::MAX - 1);
        world.set_change_cursor_for_test(base);
        let mut journal = MutationJournal::new(&world);
        for _ in 0..3 {
            journal.record_prevalidated_component_modified(entity, type_id, target);
        }
        journal.commit(&mut world);
        let final_cursor = ChangeCursor::from_parts(world.scope_id(), 5, 1);
        assert_eq!(world.current_change_cursor(), final_cursor);
        assert_eq!(
            world.archetype_component_metadata::<A>(entity).unwrap().1,
            final_cursor,
        );
        assert!(world.component_changed_since::<A>(base).unwrap());
        assert!(!world.component_changed_since::<A>(final_cursor).unwrap());
    }
}

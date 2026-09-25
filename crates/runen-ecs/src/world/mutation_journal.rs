use super::World;
use super::change_tracking::{ChangeCursor, panic_change_cursor_exhausted};
use crate::entity::Entity;
use std::any::TypeId;
use std::ptr::NonNull;
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

#[derive(Clone)]
pub(crate) struct ConcurrentMutationCapacity {
    base_cursor: ChangeCursor,
    state: Arc<Mutex<ConcurrentMutationCapacityState>>,
}

struct ConcurrentMutationCapacityState {
    remaining: u128,
    admitted: u128,
}

impl ConcurrentMutationCapacity {
    pub(crate) fn new(base_cursor: ChangeCursor) -> Self {
        let ordinal = ((base_cursor.epoch() as u128) << 64) | base_cursor.tick() as u128;
        Self {
            base_cursor,
            state: Arc::new(Mutex::new(ConcurrentMutationCapacityState {
                remaining: u128::MAX - ordinal,
                admitted: 0,
            })),
        }
    }

    fn reserve_next_event(&self) {
        let exhausted = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.admitted == state.remaining {
                true
            } else {
                state.admitted += 1;
                false
            }
        };
        if exhausted {
            panic_change_cursor_exhausted();
        }
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
        for event in self.events {
            match event {
                MutationEvent::ComponentModified {
                    entity,
                    component_type,
                    target,
                } => match target {
                    Some(target) => world.commit_prevalidated_component_mutation_event(
                        entity,
                        component_type,
                        target.changed_tick(),
                    ),
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
mod tests {
    use super::*;
    use crate::Component;
    use crate::world::change_tracking::{FrameworkInvariantKind, framework_invariant_kind};

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
        let capacity = ConcurrentMutationCapacity::new(base);

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
        let capacity = ConcurrentMutationCapacity::new(base);

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
}

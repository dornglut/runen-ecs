// Owner: RunenECS World - Runtime and Query Entry APIs
use super::{ChangeCursor, World};
use crate::commands::{Commands, LocalCommands};
use crate::errors::ChangeCursorError;
use crate::query::{QueryFilter, QuerySpec, QueryState, RemovedState};

impl World {
    pub fn commands(&self) -> Commands<'static> {
        Commands::new()
    }

    pub fn local_commands(&self) -> LocalCommands<'static> {
        LocalCommands::new()
    }

    pub fn query<Q: QuerySpec>(&self) -> QueryState<Q> {
        QueryState::new(self)
    }

    pub fn query_filtered<Q: QuerySpec, F: QueryFilter>(&self) -> QueryState<Q, F> {
        QueryState::new(self)
    }

    pub fn query_removed_state<T: crate::component::Component>(&self) -> RemovedState<T> {
        RemovedState::new(self)
    }

    pub fn current_change_cursor(&self) -> ChangeCursor {
        self.change_tick
    }

    pub(crate) fn validate_change_cursor(
        &self,
        cursor: ChangeCursor,
    ) -> Result<(), ChangeCursorError> {
        if cursor.is_from(self.scope_id()) {
            Ok(())
        } else {
            Err(ChangeCursorError::ForeignWorld)
        }
    }

    #[cfg(test)]
    pub(crate) fn set_change_cursor_for_test(&mut self, cursor: ChangeCursor) {
        assert!(
            cursor.is_from(self.scope_id()),
            "test cursor must belong to the target World"
        );
        self.change_tick = cursor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Query, Runtime};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(crate::Resource)]
    struct BoundaryResource;

    #[test]
    fn change_cursor_crosses_the_inner_boundary_without_aliasing() {
        let mut world = World::new();
        let scope = world.scope_id();
        world.set_change_cursor_for_test(ChangeCursor::from_parts(scope, 7, u64::MAX));
        let before = world.current_change_cursor();

        world.insert_resource(BoundaryResource);

        let after = world.current_change_cursor();
        assert_eq!(after, ChangeCursor::from_parts(scope, 8, 0));
        assert!(after > before);
        assert_eq!(
            world.resource_changed_since::<BoundaryResource>(before),
            Ok(true)
        );
    }

    #[test]
    fn exhausted_change_cursor_panics_instead_of_reusing_a_position() {
        let mut world = World::new();
        let scope = world.scope_id();
        world.set_change_cursor_for_test(ChangeCursor::from_parts(scope, u64::MAX, u64::MAX));
        let before = world.current_change_cursor();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.insert_resource(BoundaryResource);
        }));

        assert!(result.is_err());
        assert_eq!(world.current_change_cursor(), before);
    }

    #[derive(crate::Component)]
    struct ExhaustionA;

    #[derive(crate::Component)]
    struct ExhaustionB;

    #[derive(Copy, Clone)]
    struct JournalExhaustion;

    impl crate::ScheduleLabel for JournalExhaustion {}

    #[test]
    fn journal_rejects_first_event_before_mutable_reference_exposure() {
        let mut world = World::new();
        let entity = world.spawn(ExhaustionA).expect("spawn should succeed");
        let body_ran = Arc::new(AtomicBool::new(false));
        let body_ran_for_system = Arc::clone(&body_ran);
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            JournalExhaustion,
            move |mut query: Query<&mut ExhaustionA>| {
                let _ = query.get(entity).expect("component should exist");
                body_ran_for_system.store(true, Ordering::Relaxed);
            },
        );

        let scope = world.scope_id();
        let exhausted = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX);
        world.set_change_cursor_for_test(exhausted);
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime
                .run_schedule::<JournalExhaustion>(&mut world)
                .unwrap();
        }))
        .expect_err("cursor exhaustion must panic");

        assert_eq!(
            super::super::change_tracking::framework_invariant_kind(payload.as_ref()),
            Some(super::super::change_tracking::FrameworkInvariantKind::ChangeCursorExhausted)
        );
        assert!(!body_ran.load(Ordering::Relaxed));
        assert_eq!(world.current_change_cursor(), exhausted);
    }

    #[test]
    fn journal_reconciles_admitted_prefix_before_later_event_exhaustion() {
        let mut world = World::new();
        let entity = world
            .spawn((ExhaustionA, ExhaustionB))
            .expect("spawn should succeed");
        let body_ran = Arc::new(AtomicBool::new(false));
        let body_ran_for_system = Arc::clone(&body_ran);
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            JournalExhaustion,
            move |mut query: Query<(&mut ExhaustionA, &mut ExhaustionB)>| {
                let _ = query.get(entity).expect("components should exist");
                body_ran_for_system.store(true, Ordering::Relaxed);
            },
        );

        let scope = world.scope_id();
        let before = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX - 1);
        let exhausted = ChangeCursor::from_parts(scope, u64::MAX, u64::MAX);
        world.set_change_cursor_for_test(before);
        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime
                .run_schedule::<JournalExhaustion>(&mut world)
                .unwrap();
        }))
        .expect_err("second tuple event must exhaust the cursor");

        assert_eq!(
            super::super::change_tracking::framework_invariant_kind(payload.as_ref()),
            Some(super::super::change_tracking::FrameworkInvariantKind::ChangeCursorExhausted)
        );
        assert!(!body_ran.load(Ordering::Relaxed));
        assert_eq!(world.current_change_cursor(), exhausted);
        assert!(
            world
                .component_changed_since::<ExhaustionA>(before)
                .unwrap()
        );
        assert!(
            !world
                .component_changed_since::<ExhaustionB>(before)
                .unwrap()
        );
    }
}

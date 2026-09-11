// Owner: RunenECS World - Runtime and Query Entry APIs
use super::{ChangeCursor, World};
use crate::commands::Commands;
use crate::errors::ChangeCursorError;
use crate::query::{QueryFilter, QuerySpec, QueryState, RemovedState};

impl World {
    pub fn commands(&self) -> Commands<'static> {
        Commands::new()
    }

    pub fn query_state<Q: QuerySpec, F: QueryFilter>(&self) -> QueryState<Q, F> {
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
}

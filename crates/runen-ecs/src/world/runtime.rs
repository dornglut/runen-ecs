// Owner: RunenECS World - Runtime and Query Entry APIs
use super::{ChangeCursor, World};
use crate::commands::Commands;
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

    pub fn current_change_tick(&self) -> ChangeCursor {
        self.change_tick
    }

    #[cfg(test)]
    pub(crate) fn set_change_cursor_for_test(&mut self, cursor: ChangeCursor) {
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
        world.set_change_cursor_for_test(ChangeCursor::from_parts(7, u64::MAX));
        let before = world.current_change_tick();

        world.insert_resource(BoundaryResource);

        let after = world.current_change_tick();
        assert_eq!(after, ChangeCursor::from_parts(8, 0));
        assert!(after > before);
        assert!(world.resource_changed_since::<BoundaryResource>(before));
    }

    #[test]
    fn exhausted_change_cursor_panics_instead_of_reusing_a_position() {
        let mut world = World::new();
        world.set_change_cursor_for_test(ChangeCursor::from_parts(u64::MAX, u64::MAX));
        let before = world.current_change_tick();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.insert_resource(BoundaryResource);
        }));

        assert!(result.is_err());
        assert_eq!(world.current_change_tick(), before);
    }
}

use super::World;
use super::change_tracking::{ChangeCursor, panic_change_cursor_exhausted};
use crate::entity::Entity;
use std::any::TypeId;

enum MutationEvent {
    ComponentModified {
        entity: Entity,
        component_type: TypeId,
    },
    ResourceModified {
        resource_type: TypeId,
    },
}

/// Ordered, invocation-local mutation-observation events.
pub(crate) struct MutationJournal {
    base_cursor: ChangeCursor,
    next_reserved_cursor: Option<ChangeCursor>,
    events: Vec<MutationEvent>,
}

impl MutationJournal {
    pub(crate) fn new(world: &World) -> Self {
        Self {
            base_cursor: world.current_change_cursor(),
            next_reserved_cursor: world.current_change_cursor().next(),
            events: Vec::new(),
        }
    }

    pub(crate) fn record_component_modified(&mut self, entity: Entity, component_type: TypeId) {
        self.reserve_next_event();
        self.events.push(MutationEvent::ComponentModified {
            entity,
            component_type,
        });
    }

    pub(crate) fn record_resource_modified(&mut self, resource_type: TypeId) {
        self.reserve_next_event();
        self.events
            .push(MutationEvent::ResourceModified { resource_type });
    }

    fn reserve_next_event(&mut self) {
        let reserved = self
            .next_reserved_cursor
            .take()
            .unwrap_or_else(|| panic_change_cursor_exhausted());
        self.next_reserved_cursor = reserved.next();
    }

    pub(crate) fn commit(self, world: &mut World) {
        if self.events.is_empty() {
            return;
        }

        assert_eq!(
            world.current_change_cursor(),
            self.base_cursor,
            "mutation journal base cursor changed before reconciliation"
        );

        for event in self.events {
            match event {
                MutationEvent::ComponentModified {
                    entity,
                    component_type,
                } => world.commit_component_mutation_event(entity, component_type),
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
    use crate::world::change_tracking::{FrameworkInvariantKind, framework_invariant_kind};

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
}

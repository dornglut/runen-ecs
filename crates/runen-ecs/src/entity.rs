use crate::errors::{EntityAllocationError, EntityError};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

const WORLD_SCOPE_EXHAUSTED_MESSAGE: &str = "RunenECS world scope identity space exhausted";
static NEXT_WORLD_SCOPE: AtomicU64 = AtomicU64::new(1);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct WorldScopeId(u64);

impl WorldScopeId {
    fn allocate_process_local() -> Self {
        loop {
            let current = NEXT_WORLD_SCOPE.load(Ordering::Relaxed);
            let Some(next) = current.checked_add(1) else {
                panic!("{WORLD_SCOPE_EXHAUSTED_MESSAGE}");
            };
            if NEXT_WORLD_SCOPE
                .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Self(current);
            }
        }
    }

    #[cfg(test)]
    fn allocate_from(next: &mut u64) -> Option<Self> {
        let current = *next;
        *next = current.checked_add(1)?;
        Some(Self(current))
    }
}

/// Opaque process-local ECS entity identity.
///
/// `index()` and `generation()` are runtime diagnostics only. They are not stable
/// persistence, replay, editor, product, or network identities.
///
/// ```compile_fail
/// use runen_ecs::Entity;
/// let _ = Entity::default();
/// ```
///
/// ```compile_fail
/// use runen_ecs::Entity;
/// let _ = Entity { id: 0, generation: 0 };
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Entity {
    scope: WorldScopeId,
    index: u32,
    generation: u32,
}

impl Entity {
    const fn new(scope: WorldScopeId, index: u32, generation: u32) -> Self {
        Self {
            scope,
            index,
            generation,
        }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }

    #[cfg(test)]
    pub(crate) const fn test_only(index: u32, generation: u32) -> Self {
        Self::new(WorldScopeId(0), index, generation)
    }
}

impl fmt::Debug for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entity")
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum SlotState {
    Live,
    Free,
    Retired,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct EntitySlot {
    generation: u32,
    state: SlotState,
    last_freed_generation: Option<u32>,
}

impl EntitySlot {
    const fn live() -> Self {
        Self {
            generation: 0,
            state: SlotState::Live,
            last_freed_generation: None,
        }
    }
}

pub struct EntityAllocator {
    scope: WorldScopeId,
    next_index: u64,
    free_list: Vec<u32>,
    slots: Vec<EntitySlot>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self {
            scope: WorldScopeId::allocate_process_local(),
            next_index: 0,
            free_list: Vec::new(),
            slots: Vec::new(),
        }
    }

    pub(crate) const fn scope_id(&self) -> WorldScopeId {
        self.scope
    }

    #[cfg(test)]
    pub(crate) fn exhaust_index_space_for_test(&mut self) {
        self.next_index = u64::from(u32::MAX) + 1;
    }

    pub fn allocate(&mut self) -> Result<Entity, EntityAllocationError> {
        if let Some(index) = self.free_list.pop() {
            let slot = self
                .slots
                .get_mut(index as usize)
                .expect("free-list entity index must reference an allocated slot");
            assert!(
                matches!(slot.state, SlotState::Free),
                "free-list entity slot must be reusable"
            );
            slot.state = SlotState::Live;
            slot.last_freed_generation = None;
            return Ok(Entity::new(self.scope, index, slot.generation));
        }

        let index =
            u32::try_from(self.next_index).map_err(|_| EntityAllocationError::IndexExhausted)?;
        self.next_index += 1;
        self.slots.push(EntitySlot::live());
        Ok(Entity::new(self.scope, index, 0))
    }

    pub(crate) fn contains(&self, entity: Entity) -> bool {
        self.validate(entity).is_ok()
    }

    pub(crate) fn validate(&self, entity: Entity) -> Result<(), EntityError> {
        if entity.scope != self.scope {
            return Err(EntityError::ForeignWorld { entity });
        }

        let Some(slot) = self.slots.get(entity.index as usize) else {
            return Err(EntityError::UnknownEntity { entity });
        };

        match slot.state {
            SlotState::Live if entity.generation == slot.generation => Ok(()),
            SlotState::Live => Err(EntityError::StaleGeneration {
                entity,
                current_generation: slot.generation,
            }),
            SlotState::Free | SlotState::Retired
                if slot.last_freed_generation == Some(entity.generation) =>
            {
                Err(EntityError::AlreadyFreed { entity })
            }
            SlotState::Free | SlotState::Retired if entity.generation != slot.generation => {
                Err(EntityError::StaleGeneration {
                    entity,
                    current_generation: slot.generation,
                })
            }
            SlotState::Free | SlotState::Retired => Err(EntityError::UnknownEntity { entity }),
        }
    }

    pub fn free(&mut self, entity: Entity) -> Result<(), EntityError> {
        self.validate(entity)?;

        let slot = self
            .slots
            .get_mut(entity.index as usize)
            .expect("validated entity index must reference an allocated slot");
        let freed_generation = slot.generation;
        slot.last_freed_generation = Some(freed_generation);

        if let Some(next_generation) = freed_generation.checked_add(1) {
            slot.generation = next_generation;
            slot.state = SlotState::Free;
            self.free_list.push(entity.index);
        } else {
            slot.state = SlotState::Retired;
        }

        Ok(())
    }
}

impl Default for EntityAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    fn assert_entity_traits<T: Copy + Eq + Hash + Ord>() {}

    #[test]
    fn entity_keeps_required_value_traits() {
        assert_entity_traits::<Entity>();
    }

    #[test]
    fn world_scope_allocation_stops_before_wrap_or_reuse() {
        let mut next = u64::MAX - 1;
        assert_eq!(
            WorldScopeId::allocate_from(&mut next),
            Some(WorldScopeId(u64::MAX - 1))
        );
        assert_eq!(next, u64::MAX);
        assert_eq!(WorldScopeId::allocate_from(&mut next), None);
        assert_eq!(next, u64::MAX);
    }

    #[test]
    fn allocator_reuses_slot_only_after_generation_advance() {
        let mut allocator = EntityAllocator::new();
        let first = allocator.allocate().expect("allocation should succeed");
        allocator.free(first).expect("free should succeed");
        let second = allocator.allocate().expect("reuse should succeed");

        assert_eq!(first.index(), second.index());
        assert_eq!(second.generation(), first.generation() + 1);
        assert_ne!(first, second);
    }

    #[test]
    fn allocator_distinguishes_double_free_from_stale_generation() {
        let mut allocator = EntityAllocator::new();
        let first = allocator.allocate().expect("allocation should succeed");
        allocator.free(first).expect("free should succeed");
        let free_count = allocator.free_list.len();
        assert!(matches!(
            allocator.free(first),
            Err(EntityError::AlreadyFreed { .. })
        ));
        assert_eq!(
            allocator.free_list.len(),
            free_count,
            "rejected double free must not duplicate reusable free-list state"
        );

        let second = allocator.allocate().expect("reuse should succeed");
        assert!(matches!(
            allocator.free(first),
            Err(EntityError::StaleGeneration { .. })
        ));
        allocator.free(second).expect("current entity should free");
    }

    #[test]
    fn allocator_rejects_foreign_and_unknown_entities_without_mutation() {
        let mut first_allocator = EntityAllocator::new();
        let mut second_allocator = EntityAllocator::new();
        let local = first_allocator
            .allocate()
            .expect("allocation should succeed");
        let foreign = second_allocator
            .allocate()
            .expect("allocation should succeed");
        let next_index = first_allocator.next_index;
        let slot_count = first_allocator.slots.len();
        let free_count = first_allocator.free_list.len();

        assert!(matches!(
            first_allocator.free(foreign),
            Err(EntityError::ForeignWorld { .. })
        ));
        assert_eq!(first_allocator.next_index, next_index);
        assert_eq!(first_allocator.slots.len(), slot_count);
        assert_eq!(first_allocator.free_list.len(), free_count);
        assert!(first_allocator.contains(local));

        let unknown = Entity::new(first_allocator.scope, 99, 0);
        assert!(matches!(
            first_allocator.free(unknown),
            Err(EntityError::UnknownEntity { .. })
        ));
        assert_eq!(first_allocator.next_index, next_index);
        assert_eq!(first_allocator.slots.len(), slot_count);
        assert_eq!(first_allocator.free_list.len(), free_count);
        assert!(first_allocator.contains(local));
    }

    #[test]
    fn generation_exhaustion_retires_slot_permanently() {
        let mut allocator = EntityAllocator::new();
        let initial = allocator.allocate().expect("allocation should succeed");
        let slot = allocator
            .slots
            .get_mut(initial.index() as usize)
            .expect("slot should exist");
        slot.generation = u32::MAX;
        let exhausted = Entity::new(allocator.scope, initial.index(), u32::MAX);

        allocator
            .free(exhausted)
            .expect("terminal free should succeed");
        assert!(matches!(
            allocator.free(exhausted),
            Err(EntityError::AlreadyFreed { .. })
        ));

        let replacement = allocator.allocate().expect("new slot should allocate");
        assert_ne!(replacement.index(), exhausted.index());
        assert!(matches!(
            allocator.validate(exhausted),
            Err(EntityError::AlreadyFreed { .. })
        ));
    }

    #[test]
    fn index_exhaustion_is_structured_and_non_mutating() {
        let mut allocator = EntityAllocator::new();
        allocator.next_index = u64::from(u32::MAX) + 1;
        let slot_count = allocator.slots.len();
        let free_count = allocator.free_list.len();

        assert_eq!(
            allocator.allocate(),
            Err(EntityAllocationError::IndexExhausted)
        );
        assert_eq!(allocator.slots.len(), slot_count);
        assert_eq!(allocator.free_list.len(), free_count);
        assert_eq!(allocator.next_index, u64::from(u32::MAX) + 1);
    }
}

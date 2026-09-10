// Owner: RunenECS Storage - Entity to Archetype Location Map
use super::ArchetypeId;
use crate::entity::Entity;
use std::collections::HashMap;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct EntityLocation {
    pub(crate) archetype_id: ArchetypeId,
    pub(crate) row: usize,
}

#[derive(Debug, Default)]
pub(crate) struct EntityLocationMap {
    entries: HashMap<Entity, EntityLocation>,
}

impl EntityLocationMap {
    pub(crate) fn get(&self, entity: Entity) -> Option<EntityLocation> {
        self.entries.get(&entity).copied()
    }

    pub(crate) fn insert(&mut self, entity: Entity, location: EntityLocation) {
        self.entries.insert(entity, location);
    }

    pub(crate) fn remove(&mut self, entity: Entity) -> Option<EntityLocation> {
        self.entries.remove(&entity)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_map_insert_lookup_and_remove() {
        let mut map = EntityLocationMap::default();
        let entity = Entity::test_only(10, 0);
        let location = EntityLocation {
            archetype_id: ArchetypeId::new(2),
            row: 4,
        };

        assert!(map.get(entity).is_none());
        map.insert(entity, location);
        assert_eq!(map.get(entity), Some(location));
        assert_eq!(map.remove(entity), Some(location));
        assert!(map.get(entity).is_none());
    }
}

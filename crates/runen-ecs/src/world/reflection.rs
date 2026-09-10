use crate::reflect::TypeRegistry;
use crate::world::World;

impl World {
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }
}

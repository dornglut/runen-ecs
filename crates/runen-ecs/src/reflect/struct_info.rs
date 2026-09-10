//! File: crates/runen-ecs/src/reflect/struct_info.rs
//! Purpose: Reflected struct shape metadata.

use crate::reflect::FieldInfo;

pub type StructFieldAt = fn(usize) -> Option<FieldInfo>;

#[derive(Debug, Clone, Copy)]
pub struct StructInfo {
    field_count: usize,
    field_at: StructFieldAt,
}

impl StructInfo {
    pub const fn new(field_count: usize, field_at: StructFieldAt) -> Self {
        Self {
            field_count,
            field_at,
        }
    }

    pub fn field_count(&self) -> usize {
        self.field_count
    }

    pub fn fields(&self) -> impl Iterator<Item = FieldInfo> + '_ {
        (0..self.field_count).filter_map(|index| self.field_at(index))
    }

    pub fn field_at(&self, index: usize) -> Option<FieldInfo> {
        (self.field_at)(index)
    }

    pub fn field_named(&self, name: &str) -> Option<FieldInfo> {
        self.fields().find(|field| field.name == name)
    }
}

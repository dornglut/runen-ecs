//! File: crates/runen-ecs/src/reflect/value.rs
//! Purpose: Dynamic reflected value access.

use crate::reflect::{EnumInfo, EnumVariantInfo, FieldInfo, Reflect, StructInfo, TypeInfo};

#[derive(Clone, Copy)]
pub struct ReflectValueRef<'a> {
    pub type_info: TypeInfo,
    pub value: &'a dyn std::any::Any,
}

pub struct ReflectValueMut<'a> {
    pub type_info: TypeInfo,
    pub value: &'a mut dyn std::any::Any,
}

impl<'a> core::fmt::Debug for ReflectValueRef<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReflectValueRef")
            .field("type_info", &self.type_info)
            .finish()
    }
}

impl<'a> core::fmt::Debug for ReflectValueMut<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReflectValueMut")
            .field("type_info", &self.type_info)
            .finish()
    }
}

impl<'a> ReflectValueRef<'a> {
    pub fn new<T>(value: &'a T) -> Self
    where
        T: Reflect,
    {
        Self {
            type_info: T::type_info(),
            value,
        }
    }

    pub fn type_info(&self) -> TypeInfo {
        self.type_info
    }

    pub fn as_any(&self) -> &'a dyn std::any::Any {
        self.value
    }

    pub fn downcast_ref<T: 'static>(&self) -> Option<&'a T> {
        self.value.downcast_ref::<T>()
    }

    pub fn struct_ref(&self) -> Option<StructValueRef<'a>> {
        self.type_info.struct_info().map(|info| StructValueRef {
            info,
            owner: self.value,
        })
    }

    pub fn enum_ref(&self) -> Option<EnumValueRef<'a>> {
        self.type_info.enum_info().map(|info| EnumValueRef {
            info,
            owner: self.value,
        })
    }
}

impl<'a> ReflectValueMut<'a> {
    pub fn new<T>(value: &'a mut T) -> Self
    where
        T: Reflect,
    {
        Self {
            type_info: T::type_info(),
            value,
        }
    }

    pub fn type_info(&self) -> TypeInfo {
        self.type_info
    }

    pub fn as_any(&self) -> &dyn std::any::Any {
        self.value
    }

    pub fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self.value
    }

    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }

    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.value.downcast_mut::<T>()
    }

    pub fn struct_mut(self) -> Option<StructValueMut<'a>> {
        self.type_info.struct_info().map(|info| StructValueMut {
            info,
            owner: self.value,
        })
    }

    pub fn enum_mut(self) -> Option<EnumValueMut<'a>> {
        self.type_info.enum_info().map(|info| EnumValueMut {
            info,
            owner: self.value,
        })
    }
}

#[derive(Clone, Copy)]
pub struct StructValueRef<'a> {
    pub info: StructInfo,
    owner: &'a dyn std::any::Any,
}

pub struct StructValueMut<'a> {
    pub info: StructInfo,
    owner: &'a mut dyn std::any::Any,
}

#[derive(Clone, Copy)]
pub struct EnumValueRef<'a> {
    pub info: EnumInfo,
    owner: &'a dyn std::any::Any,
}

pub struct EnumValueMut<'a> {
    pub info: EnumInfo,
    owner: &'a mut dyn std::any::Any,
}

impl<'a> core::fmt::Debug for StructValueRef<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StructValueRef")
            .field("field_count", &self.info.field_count())
            .finish()
    }
}

impl<'a> core::fmt::Debug for StructValueMut<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StructValueMut")
            .field("field_count", &self.info.field_count())
            .finish()
    }
}

impl<'a> core::fmt::Debug for EnumValueRef<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EnumValueRef")
            .field("variant_count", &self.info.variant_count())
            .finish()
    }
}

impl<'a> core::fmt::Debug for EnumValueMut<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EnumValueMut")
            .field("variant_count", &self.info.variant_count())
            .finish()
    }
}

impl<'a> StructValueRef<'a> {
    pub fn fields(&self) -> impl Iterator<Item = FieldInfo> + '_ {
        self.info.fields()
    }

    pub fn field(&self, name: &str) -> Option<ReflectValueRef<'a>> {
        let field = self.info.field_named(name)?;
        (field.get_ref)(self.owner)
    }

    pub fn field_at(&self, index: usize) -> Option<ReflectValueRef<'a>> {
        let field = self.info.field_at(index)?;
        (field.get_ref)(self.owner)
    }
}

impl<'a> StructValueMut<'a> {
    pub fn fields(&self) -> impl Iterator<Item = FieldInfo> + '_ {
        self.info.fields()
    }

    pub fn field_mut(&mut self, name: &str) -> Option<ReflectValueMut<'_>> {
        let field = self.info.field_named(name)?;
        (field.get_mut)(self.owner)
    }

    pub fn field_at_mut(&mut self, index: usize) -> Option<ReflectValueMut<'_>> {
        let field = self.info.field_at(index)?;
        (field.get_mut)(self.owner)
    }
}

impl<'a> EnumValueRef<'a> {
    pub fn variants(&self) -> Vec<EnumVariantInfo> {
        self.info.variants()
    }

    pub fn current_symbol(&self) -> Option<&'static str> {
        (self.info.current_variant)(self.owner)
    }
}

impl<'a> EnumValueMut<'a> {
    pub fn variants(&self) -> Vec<EnumVariantInfo> {
        self.info.variants()
    }

    pub fn current_symbol(&self) -> Option<&'static str> {
        (self.info.current_variant)(self.owner)
    }

    pub fn set_unit_variant(&mut self, symbol: &str) -> bool {
        (self.info.set_unit_variant)(self.owner, symbol)
    }
}

// Owner: RunenECS - Query Runtime
use crate::component::{Component, Resource};
use crate::entity::Entity;
use crate::world::{ChangeCursor, QueryCapability};
use std::any::TypeId;
use std::marker::PhantomData;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct QueryTypeAccess {
    type_id: TypeId,
    name: &'static str,
}

impl QueryTypeAccess {
    pub fn of<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
        }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum QueryBorrowMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct QueryBorrowAccess {
    type_id: TypeId,
    name: &'static str,
    mode: QueryBorrowMode,
}

impl QueryBorrowAccess {
    fn shared<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
            mode: QueryBorrowMode::Shared,
        }
    }

    fn exclusive<T: 'static>(name: &'static str) -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name,
            mode: QueryBorrowMode::Exclusive,
        }
    }

    fn conflicts_with(self, other: Self) -> bool {
        self.type_id == other.type_id
            && (matches!(self.mode, QueryBorrowMode::Exclusive)
                || matches!(other.mode, QueryBorrowMode::Exclusive))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct QueryBorrowConflict {
    domain: &'static str,
    name: &'static str,
}

impl QueryBorrowConflict {
    pub(crate) const fn domain(self) -> &'static str {
        self.domain
    }

    pub(crate) const fn name(self) -> &'static str {
        self.name
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueryAccess {
    component_reads: Vec<QueryTypeAccess>,
    removed_component_reads: Vec<QueryTypeAccess>,
    component_writes: Vec<QueryTypeAccess>,
    resource_reads: Vec<QueryTypeAccess>,
    resource_writes: Vec<QueryTypeAccess>,
    deferred_structural_mutation: bool,
    exclusive_world_accesses: usize,
    component_borrows: Vec<QueryBorrowAccess>,
    resource_borrows: Vec<QueryBorrowAccess>,
}

impl QueryAccess {
    pub fn structural_mutation() -> Self {
        Self {
            deferred_structural_mutation: true,
            ..Self::default()
        }
    }

    pub fn exclusive_world() -> Self {
        Self {
            exclusive_world_accesses: 1,
            ..Self::default()
        }
    }

    pub fn component_reads(&self) -> &[QueryTypeAccess] {
        &self.component_reads
    }

    pub fn component_writes(&self) -> &[QueryTypeAccess] {
        &self.component_writes
    }

    pub fn removed_component_reads(&self) -> &[QueryTypeAccess] {
        &self.removed_component_reads
    }

    pub fn resource_reads(&self) -> &[QueryTypeAccess] {
        &self.resource_reads
    }

    pub fn resource_writes(&self) -> &[QueryTypeAccess] {
        &self.resource_writes
    }

    pub fn with_component_read<T: Component>(mut self) -> Self {
        self.add_component_read::<T>();
        self
    }

    pub fn with_component_write<T: Component>(mut self) -> Self {
        self.add_component_write::<T>();
        self
    }

    pub fn with_removed_component_read<T: Component>(mut self) -> Self {
        self.add_removed_component_read::<T>();
        self
    }

    pub fn with_resource_read<T: Resource>(mut self) -> Self {
        self.add_resource_read::<T>();
        self
    }

    pub fn with_resource_write<T: Resource>(mut self) -> Self {
        self.add_resource_write::<T>();
        self
    }

    pub fn deferred_structural_mutation(&self) -> bool {
        self.deferred_structural_mutation
    }

    pub fn exclusive_world_accesses(&self) -> usize {
        self.exclusive_world_accesses
    }

    pub(crate) fn add_component_read<T: Component>(&mut self) {
        self.component_borrows
            .push(QueryBorrowAccess::shared::<T>(T::component_name()));
        push_unique_access(
            &mut self.component_reads,
            QueryTypeAccess::of::<T>(T::component_name()),
        );
    }

    pub(crate) fn add_component_write<T: Component>(&mut self) {
        self.component_borrows
            .push(QueryBorrowAccess::exclusive::<T>(T::component_name()));
        push_unique_access(
            &mut self.component_writes,
            QueryTypeAccess::of::<T>(T::component_name()),
        );
    }

    pub(crate) fn add_removed_component_read<T: Component>(&mut self) {
        push_unique_access(
            &mut self.removed_component_reads,
            QueryTypeAccess::of::<T>(T::component_name()),
        );
    }

    pub(crate) fn add_resource_read<T: Resource>(&mut self) {
        self.resource_borrows
            .push(QueryBorrowAccess::shared::<T>(T::resource_name()));
        push_unique_access(
            &mut self.resource_reads,
            QueryTypeAccess::of::<T>(T::resource_name()),
        );
    }

    pub(crate) fn add_resource_write<T: Resource>(&mut self) {
        self.resource_borrows
            .push(QueryBorrowAccess::exclusive::<T>(T::resource_name()));
        push_unique_access(
            &mut self.resource_writes,
            QueryTypeAccess::of::<T>(T::resource_name()),
        );
    }

    pub(crate) fn borrow_checkpoint(&self) -> (usize, usize) {
        (self.component_borrows.len(), self.resource_borrows.len())
    }

    pub(crate) fn restore_borrow_checkpoint(&mut self, checkpoint: (usize, usize)) {
        self.component_borrows.truncate(checkpoint.0);
        self.resource_borrows.truncate(checkpoint.1);
    }

    pub(crate) fn borrow_conflict(&self) -> Option<QueryBorrowConflict> {
        if self.exclusive_world_accesses > 1
            || (self.exclusive_world_accesses == 1 && self.has_immediate_world_access())
        {
            return Some(QueryBorrowConflict {
                domain: "world",
                name: "exclusive world",
            });
        }

        find_borrow_conflict("component", &self.component_borrows)
            .or_else(|| find_borrow_conflict("resource", &self.resource_borrows))
    }

    fn has_immediate_world_access(&self) -> bool {
        !self.component_reads.is_empty()
            || !self.removed_component_reads.is_empty()
            || !self.component_writes.is_empty()
            || !self.resource_reads.is_empty()
            || !self.resource_writes.is_empty()
    }

    /// Extends this access set with another access set.
    ///
    /// Composite `SystemParam` implementations use this as the canonical way to
    /// preserve child access semantics while reporting one grouped parameter.
    pub fn extend(&mut self, other: Self) {
        self.component_borrows.extend(other.component_borrows);
        self.resource_borrows.extend(other.resource_borrows);
        for access in other.component_reads {
            push_unique_access(&mut self.component_reads, access);
        }
        for access in other.removed_component_reads {
            push_unique_access(&mut self.removed_component_reads, access);
        }
        for access in other.component_writes {
            push_unique_access(&mut self.component_writes, access);
        }
        for access in other.resource_reads {
            push_unique_access(&mut self.resource_reads, access);
        }
        for access in other.resource_writes {
            push_unique_access(&mut self.resource_writes, access);
        }
        self.deferred_structural_mutation |= other.deferred_structural_mutation;
        self.exclusive_world_accesses = self
            .exclusive_world_accesses
            .saturating_add(other.exclusive_world_accesses);
    }
}

mod sealed {
    pub trait QueryFilterSealed {}
}

pub trait QueryFilter: sealed::QueryFilterSealed {
    fn configure(required: &mut Vec<TypeId>, excluded: &mut Vec<TypeId>);

    fn append_access(_access: &mut QueryAccess) {}

    fn needs_tick_filter() -> bool {
        false
    }

    fn matches_entity(
        _world: QueryCapability<'_>,
        _entity: Entity,
        _since_tick: ChangeCursor,
    ) -> bool {
        true
    }
}

impl QueryFilter for () {
    fn configure(_required: &mut Vec<TypeId>, _excluded: &mut Vec<TypeId>) {}
}

pub struct With<T: Component>(PhantomData<T>);
pub struct Without<T: Component>(PhantomData<T>);

impl<T: Component> QueryFilter for With<T> {
    fn configure(required: &mut Vec<TypeId>, _excluded: &mut Vec<TypeId>) {
        push_unique_type(required, TypeId::of::<T>());
    }
}

impl<T: Component> QueryFilter for Without<T> {
    fn configure(_required: &mut Vec<TypeId>, excluded: &mut Vec<TypeId>) {
        push_unique_type(excluded, TypeId::of::<T>());
    }
}

pub struct Changed<T: Component>(PhantomData<T>);
pub struct Added<T: Component>(PhantomData<T>);

impl sealed::QueryFilterSealed for () {}
impl<T: Component> sealed::QueryFilterSealed for With<T> {}
impl<T: Component> sealed::QueryFilterSealed for Without<T> {}
impl<T: Component> sealed::QueryFilterSealed for Changed<T> {}
impl<T: Component> sealed::QueryFilterSealed for Added<T> {}
impl<A: QueryFilter, B: QueryFilter> sealed::QueryFilterSealed for (A, B) {}

macro_rules! impl_query_filter_sealed_tuple {
    ($(($($name:ident),+)),+ $(,)?) => {$(impl<$($name: QueryFilter),+> sealed::QueryFilterSealed for ($($name,)+) {})+};
}
impl_query_filter_sealed_tuple!((A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F));

impl<T: Component> QueryFilter for Changed<T> {
    fn configure(required: &mut Vec<TypeId>, _excluded: &mut Vec<TypeId>) {
        push_unique_type(required, TypeId::of::<T>());
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<T>();
    }

    fn needs_tick_filter() -> bool {
        true
    }

    fn matches_entity(
        world: QueryCapability<'_>,
        entity: Entity,
        since_tick: ChangeCursor,
    ) -> bool {
        world.component_changed_for_entity_since::<T>(entity, since_tick)
    }
}

impl<T: Component> QueryFilter for Added<T> {
    fn configure(required: &mut Vec<TypeId>, _excluded: &mut Vec<TypeId>) {
        push_unique_type(required, TypeId::of::<T>());
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<T>();
    }

    fn needs_tick_filter() -> bool {
        true
    }

    fn matches_entity(
        world: QueryCapability<'_>,
        entity: Entity,
        since_tick: ChangeCursor,
    ) -> bool {
        world.component_added_for_entity_since::<T>(entity, since_tick)
    }
}

impl<A: QueryFilter, B: QueryFilter> QueryFilter for (A, B) {
    fn configure(required: &mut Vec<TypeId>, excluded: &mut Vec<TypeId>) {
        A::configure(required, excluded);
        B::configure(required, excluded);
    }

    fn append_access(access: &mut QueryAccess) {
        A::append_access(access);
        B::append_access(access);
    }

    fn needs_tick_filter() -> bool {
        A::needs_tick_filter() || B::needs_tick_filter()
    }

    fn matches_entity(
        world: QueryCapability<'_>,
        entity: Entity,
        since_tick: ChangeCursor,
    ) -> bool {
        A::matches_entity(world, entity, since_tick) && B::matches_entity(world, entity, since_tick)
    }
}

macro_rules! impl_query_filter_tuple {
    ($(($($name:ident),+)),+ $(,)?) => {
        $(
            impl<$($name: QueryFilter,)+> QueryFilter for ($($name,)+) {
                fn configure(required: &mut Vec<TypeId>, excluded: &mut Vec<TypeId>) {
                    $(
                        $name::configure(required, excluded);
                    )+
                }

                fn append_access(access: &mut QueryAccess) {
                    $(
                        $name::append_access(access);
                    )+
                }

                fn needs_tick_filter() -> bool {
                    false $(|| $name::needs_tick_filter())+
                }

                fn matches_entity(world: QueryCapability<'_>, entity: Entity, since_tick: ChangeCursor) -> bool {
                    true $(
                        && $name::matches_entity(world, entity, since_tick)
                    )+
                }
            }
        )+
    };
}

impl_query_filter_tuple!((A, B, C), (A, B, C, D), (A, B, C, D, E), (A, B, C, D, E, F));

impl<T: Component> Default for With<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> With<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: Component> Default for Without<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> Without<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: Component> Default for Changed<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> Changed<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: Component> Default for Added<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> Added<T> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

pub(super) fn push_unique_type(target: &mut Vec<TypeId>, type_id: TypeId) {
    if !target.contains(&type_id) {
        target.push(type_id);
    }
}

fn push_unique_access(target: &mut Vec<QueryTypeAccess>, access: QueryTypeAccess) {
    if !target.iter().any(|entry| entry.type_id == access.type_id) {
        target.push(access);
    }
}

fn find_borrow_conflict(
    domain: &'static str,
    borrows: &[QueryBorrowAccess],
) -> Option<QueryBorrowConflict> {
    for (index, left) in borrows.iter().copied().enumerate() {
        for right in borrows.iter().copied().skip(index + 1) {
            if left.conflicts_with(right) {
                return Some(QueryBorrowConflict {
                    domain,
                    name: left.name,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod exclusive_world_tests {
    use super::QueryAccess;
    use crate::Resource;

    struct ResourceA;
    impl Resource for ResourceA {}

    #[test]
    fn exclusive_world_conflicts_with_immediate_resource_borrow() {
        let mut access = QueryAccess::exclusive_world();
        access.add_resource_read::<ResourceA>();

        let conflict = access
            .borrow_conflict()
            .expect("exclusive world plus resource access must conflict");
        assert_eq!(conflict.domain(), "world");
        assert_eq!(conflict.name(), "exclusive world");
    }

    #[test]
    fn duplicate_exclusive_world_accesses_conflict() {
        let mut access = QueryAccess::exclusive_world();
        access.extend(QueryAccess::exclusive_world());

        assert!(access.borrow_conflict().is_some());
    }

    #[test]
    fn exclusive_world_can_coexist_with_deferred_structural_mutation() {
        let mut access = QueryAccess::exclusive_world();
        access.extend(QueryAccess::structural_mutation());

        assert!(access.borrow_conflict().is_none());
    }
}

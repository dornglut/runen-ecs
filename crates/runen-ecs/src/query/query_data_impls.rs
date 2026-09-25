// Owner: RunenECS - Query Runtime
use super::access_and_filters::QueryAccess;
use super::traits_and_state::{QueryArchetypeBinding, QueryData, TransferableQueryData};
use crate::component::Component;
use crate::entity::Entity;
use crate::world::{QueryCapability, WorkerWorldBuilder};
use std::any::TypeId;

impl<T: Component> QueryData for &T {
    type Item<'w> = &'w T;

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<T>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        world.component::<T>(entity)
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let ptr = binding.component_ptr_at::<T>(0, row)?;
        Some(unsafe { &*ptr })
    }
}

impl<T: Component> QueryData for &mut T {
    type Item<'w> = &'w mut T;

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<T>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        world.mark_component_modified_by_id(entity, TypeId::of::<T>());
    }

    fn mark_changed_archetype_row(
        world: QueryCapability<'_>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) {
        let entity = binding
            .entity_at(row)
            .expect("validated mutable query row must contain an entity");
        world.mark_serial_query_component_modified(
            entity,
            TypeId::of::<T>(),
            binding.changed_tick_ptr_at(0, row),
        );
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        unsafe { world.component_mut::<T>(entity) }
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let ptr = unsafe { binding.component_mut_ptr_at::<T>(0, row) }?;
        Some(unsafe { &mut *ptr })
    }
}

impl<T: Component> QueryData for (Entity, &T) {
    type Item<'w> = (Entity, &'w T);

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<T>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        world.component::<T>(entity).map(|value| (entity, value))
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let entity = binding.entity_at(row)?;
        let ptr = binding.component_ptr_at::<T>(0, row)?;
        Some((entity, unsafe { &*ptr }))
    }
}

impl<T: Component> QueryData for (Entity, &mut T) {
    type Item<'w> = (Entity, &'w mut T);

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<T>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        world.mark_component_modified_by_id(entity, TypeId::of::<T>());
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        unsafe { world.component_mut::<T>(entity) }.map(|value| (entity, value))
    }
}

impl<A: Component, B: Component> QueryData for (&A, &B) {
    type Item<'w> = (&'w A, &'w B);

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<A>();
        access.add_component_read::<B>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        let a = world.component::<A>(entity)?;
        let b = world.component::<B>(entity)?;
        Some((a, b))
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let a = binding.component_ptr_at::<A>(0, row)?;
        let b = binding.component_ptr_at::<B>(1, row)?;
        Some(unsafe { (&*a, &*b) })
    }
}

impl<A: Component, B: Component> QueryData for (&mut A, &B) {
    type Item<'w> = (&'w mut A, &'w B);

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<A>();
        access.add_component_read::<B>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        world.mark_component_modified_by_id(entity, TypeId::of::<A>());
    }

    fn mark_changed_archetype_row(
        world: QueryCapability<'_>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) {
        let entity = binding
            .entity_at(row)
            .expect("validated mutable/read query row must contain an entity");
        world.mark_serial_query_component_modified(
            entity,
            TypeId::of::<A>(),
            binding.changed_tick_ptr_at(0, row),
        );
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "mutable/read query requires distinct component types",
        );

        let world_mut = world;
        let b = world_mut.component::<B>(entity)? as *const B;
        let a = unsafe { world_mut.component_mut::<A>(entity) }? as *mut A;

        // Safety: mutable/read query access requires distinct component types.
        Some(unsafe { (&mut *a, &*b) })
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let a = unsafe { binding.component_mut_ptr_at::<A>(0, row) }?;
        let b = binding.component_ptr_at::<B>(1, row)?;
        Some(unsafe { (&mut *a, &*b) })
    }
}

impl<A: Component, B: Component> QueryData for (&A, &mut B) {
    type Item<'w> = (&'w A, &'w mut B);

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<A>();
        access.add_component_write::<B>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        world.mark_component_modified_by_id(entity, TypeId::of::<B>());
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "read/mutable query requires distinct component types",
        );

        let world_mut = world;
        let a = world_mut.component::<A>(entity)? as *const A;
        let b = unsafe { world_mut.component_mut::<B>(entity) }? as *mut B;

        // Safety: read/mutable query access requires distinct component types.
        Some(unsafe { (&*a, &mut *b) })
    }
}

impl<A: Component, B: Component> QueryData for (&mut A, &mut B) {
    type Item<'w> = (&'w mut A, &'w mut B);

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn supports_contiguous_segments() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<A>();
        access.add_component_write::<B>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        let world_mut = world;
        world_mut.mark_component_modified::<A>(entity);
        world_mut.mark_component_modified::<B>(entity);
    }

    fn mark_changed_archetype_row(
        world: QueryCapability<'_>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) {
        let entity = binding
            .entity_at(row)
            .expect("validated double-mutable query row must contain an entity");
        world.mark_serial_query_component_modified(
            entity,
            TypeId::of::<A>(),
            binding.changed_tick_ptr_at(0, row),
        );
        world.mark_serial_query_component_modified(
            entity,
            TypeId::of::<B>(),
            binding.changed_tick_ptr_at(1, row),
        );
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "double mutable query requires distinct component types",
        );

        let world_mut = world;
        let a = unsafe { world_mut.component_mut::<A>(entity) }? as *mut A;
        let b = unsafe { world_mut.component_mut::<B>(entity) }? as *mut B;

        // Safety: double mutable query access requires distinct component types.
        Some(unsafe { (&mut *a, &mut *b) })
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let a = unsafe { binding.component_mut_ptr_at::<A>(0, row) }?;
        let b = unsafe { binding.component_mut_ptr_at::<B>(1, row) }?;
        Some(unsafe { (&mut *a, &mut *b) })
    }
}

impl<T: Component> QueryData for Option<&T> {
    type Item<'w> = Option<&'w T>;

    fn query_types() -> Vec<TypeId> {
        Vec::new()
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<T>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        Some(world.component::<T>(entity))
    }
}

impl<T: Component> QueryData for Option<&mut T> {
    type Item<'w> = Option<&'w mut T>;

    fn query_types() -> Vec<TypeId> {
        Vec::new()
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<T>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        let should_mark = world.component::<T>(entity);
        if should_mark.is_some() {
            // Safety: query execution ensures exclusive mutable world access for this query form.
            world.mark_component_modified_by_id(entity, TypeId::of::<T>());
        }
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        let value = unsafe { world.component_mut::<T>(entity) };
        Some(value)
    }
}

impl<A: Component, B: Component> QueryData for (&mut A, Option<&B>) {
    type Item<'w> = (&'w mut A, Option<&'w B>);

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<A>();
        access.add_component_read::<B>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        world.mark_component_modified_by_id(entity, TypeId::of::<A>());
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "mutable/optional query requires distinct component types",
        );

        let world_mut = world;
        let b = world_mut
            .component::<B>(entity)
            .map(|value| value as *const B);
        let a = unsafe { world_mut.component_mut::<A>(entity) }? as *mut A;

        // Safety: mutable/optional query access requires distinct component types.
        Some(unsafe { (&mut *a, b.map(|ptr| &*ptr)) })
    }
}

impl<A: Component, B: Component> QueryData for (&A, Option<&B>) {
    type Item<'w> = (&'w A, Option<&'w B>);

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<A>();
        access.add_component_read::<B>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        let world_ref = world;
        let a = world_ref.component::<A>(entity)?;
        let b = world_ref.component::<B>(entity);
        Some((a, b))
    }
}

impl<A: Component, B: Component> QueryData for (&A, Option<&mut B>) {
    type Item<'w> = (&'w A, Option<&'w mut B>);

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<A>();
        access.add_component_write::<B>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        let should_mark = world.component::<B>(entity);
        if should_mark.is_some() {
            // Safety: query execution ensures exclusive mutable world access for this query form.
            world.mark_component_modified_by_id(entity, TypeId::of::<B>());
        }
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "read/optional mutable query requires distinct component types",
        );

        let world_mut = world;
        let a = world_mut.component::<A>(entity)? as *const A;
        let b = unsafe { world_mut.component_mut::<B>(entity) }.map(|value| value as *mut B);

        // Safety: read/optional mutable query access requires distinct component types.
        Some(unsafe { (&*a, b.map(|ptr| &mut *ptr)) })
    }
}

impl<A: Component, B: Component> QueryData for (&mut A, Option<&mut B>) {
    type Item<'w> = (&'w mut A, Option<&'w mut B>);

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<A>();
        access.add_component_write::<B>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        let world_mut = world;
        world_mut.mark_component_modified::<A>(entity);
        world_mut.mark_component_modified::<B>(entity);
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "mutable/optional mutable query requires distinct component types",
        );

        let world_mut = world;
        let b = unsafe { world_mut.component_mut::<B>(entity) }.map(|value| value as *mut B);
        let a = unsafe { world_mut.component_mut::<A>(entity) }? as *mut A;

        // Safety: mutable/optional mutable query access requires distinct component types.
        Some(unsafe { (&mut *a, b.map(|ptr| &mut *ptr)) })
    }
}

impl<T: Component> QueryData for (Entity, Option<&T>) {
    type Item<'w> = (Entity, Option<&'w T>);

    fn query_types() -> Vec<TypeId> {
        Vec::new()
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<T>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        let value = world.component::<T>(entity);
        Some((entity, value))
    }
}

impl<A: Component, B: Component, C: Component> QueryData for (&A, &B, &C) {
    type Item<'w> = (&'w A, &'w B, &'w C);

    fn supports_serial_archetype_spans() -> bool {
        true
    }

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_read::<A>();
        access.add_component_read::<B>();
        access.add_component_read::<C>();
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        let a = world.component::<A>(entity)?;
        let b = world.component::<B>(entity)?;
        let c = world.component::<C>(entity)?;
        Some((a, b, c))
    }

    unsafe fn fetch_archetype_row<'w>(
        _world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let a = binding.component_ptr_at::<A>(0, row)?;
        let b = binding.component_ptr_at::<B>(1, row)?;
        let c = binding.component_ptr_at::<C>(2, row)?;
        Some(unsafe { (&*a, &*b, &*c) })
    }
}

impl<A: Component, B: Component, C: Component> QueryData for (&mut A, &B, &C) {
    type Item<'w> = (&'w mut A, &'w B, &'w C);

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<A>();
        access.add_component_read::<B>();
        access.add_component_read::<C>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        world.mark_component_modified_by_id(entity, TypeId::of::<A>());
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "mutable/read tuple query requires distinct component types",
        );
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<C>(),
            "mutable/read tuple query requires distinct component types",
        );

        let world_mut = world;
        let b = world_mut.component::<B>(entity)? as *const B;
        let c = world_mut.component::<C>(entity)? as *const C;
        let a = unsafe { world_mut.component_mut::<A>(entity) }? as *mut A;

        // Safety: mutable/read tuple query access requires distinct component types.
        Some(unsafe { (&mut *a, &*b, &*c) })
    }
}

impl<A: Component, B: Component, C: Component> QueryData for (&mut A, &mut B, &C) {
    type Item<'w> = (&'w mut A, &'w mut B, &'w C);

    fn query_types() -> Vec<TypeId> {
        vec![TypeId::of::<A>(), TypeId::of::<B>(), TypeId::of::<C>()]
    }

    fn append_access(access: &mut QueryAccess) {
        access.add_component_write::<A>();
        access.add_component_write::<B>();
        access.add_component_read::<C>();
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        // Safety: query execution ensures exclusive mutable world access for this query form.
        let world_mut = world;
        world_mut.mark_component_modified::<A>(entity);
        world_mut.mark_component_modified::<B>(entity);
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<B>(),
            "mutable tuple query requires distinct component types",
        );
        assert_ne!(
            TypeId::of::<A>(),
            TypeId::of::<C>(),
            "mutable/read tuple query requires distinct component types",
        );
        assert_ne!(
            TypeId::of::<B>(),
            TypeId::of::<C>(),
            "mutable/read tuple query requires distinct component types",
        );

        let world_mut = world;
        let c = world_mut.component::<C>(entity)? as *const C;
        let a = unsafe { world_mut.component_mut::<A>(entity) }? as *mut A;
        let b = unsafe { world_mut.component_mut::<B>(entity) }? as *mut B;

        // Safety: mutable/read tuple query access requires distinct component types.
        Some(unsafe { (&mut *a, &mut *b, &*c) })
    }
}

unsafe impl<T: Component + Sync> TransferableQueryData for &T {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<T>();
    }
}
unsafe impl<T: Component + Send> TransferableQueryData for &mut T {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<T>();
    }
}
unsafe impl<T: Component + Sync> TransferableQueryData for (Entity, &T) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<T>();
    }
}
unsafe impl<T: Component + Send> TransferableQueryData for (Entity, &mut T) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<T>();
    }
}

unsafe impl<A: Component + Sync, B: Component + Sync> TransferableQueryData for (&A, &B) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<A>();
        builder.prepare_component_read::<B>();
    }
}
unsafe impl<A: Component + Send, B: Component + Sync> TransferableQueryData for (&mut A, &B) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<A>();
        builder.prepare_component_read::<B>();
    }
}
unsafe impl<A: Component + Sync, B: Component + Send> TransferableQueryData for (&A, &mut B) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<A>();
        builder.prepare_component_write::<B>();
    }
}
unsafe impl<A: Component + Send, B: Component + Send> TransferableQueryData for (&mut A, &mut B) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<A>();
        builder.prepare_component_write::<B>();
    }
}

unsafe impl<T: Component + Sync> TransferableQueryData for Option<&T> {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<T>();
    }
}
unsafe impl<T: Component + Send> TransferableQueryData for Option<&mut T> {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<T>();
    }
}
unsafe impl<A: Component + Send, B: Component + Sync> TransferableQueryData
    for (&mut A, Option<&B>)
{
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<A>();
        builder.prepare_component_read::<B>();
    }
}
unsafe impl<A: Component + Sync, B: Component + Sync> TransferableQueryData for (&A, Option<&B>) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<A>();
        builder.prepare_component_read::<B>();
    }
}
unsafe impl<A: Component + Sync, B: Component + Send> TransferableQueryData
    for (&A, Option<&mut B>)
{
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<A>();
        builder.prepare_component_write::<B>();
    }
}
unsafe impl<A: Component + Send, B: Component + Send> TransferableQueryData
    for (&mut A, Option<&mut B>)
{
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<A>();
        builder.prepare_component_write::<B>();
    }
}
unsafe impl<T: Component + Sync> TransferableQueryData for (Entity, Option<&T>) {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<T>();
    }
}

unsafe impl<A: Component + Sync, B: Component + Sync, C: Component + Sync> TransferableQueryData
    for (&A, &B, &C)
{
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_read::<A>();
        builder.prepare_component_read::<B>();
        builder.prepare_component_read::<C>();
    }
}
unsafe impl<A: Component + Send, B: Component + Sync, C: Component + Sync> TransferableQueryData
    for (&mut A, &B, &C)
{
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<A>();
        builder.prepare_component_read::<B>();
        builder.prepare_component_read::<C>();
    }
}
unsafe impl<A: Component + Send, B: Component + Send, C: Component + Sync> TransferableQueryData
    for (&mut A, &mut B, &C)
{
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>) {
        builder.prepare_component_write::<A>();
        builder.prepare_component_write::<B>();
        builder.prepare_component_read::<C>();
    }
}

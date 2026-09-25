// Owner: RunenECS - Query Runtime
use super::access_and_filters::{QueryAccess, QueryFilter, push_unique_type};
use super::contiguous::ContiguousSegments;
use crate::component::Component;
use crate::entity::{Entity, WorldScopeId};
use crate::errors::{ContiguousQueryError, QueryError};
use crate::storage::ContiguousArchetypeSpan;
use crate::world::{ChangeCursor, QueryCapability, WorkerWorldBuilder, World};
use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::ptr::NonNull;

pub trait QueryData {
    type Item<'w>;

    fn query_types() -> Vec<TypeId>;
    fn append_access(access: &mut QueryAccess);

    fn mark_changed(_world: QueryCapability<'_>, _entity: Entity) {}

    /// Enables direct serial row iteration from a validated archetype/span
    /// projection instead of materializing entity or row scratch lists.
    fn supports_serial_archetype_spans() -> bool {
        false
    }

    fn mark_changed_archetype_row(
        world: QueryCapability<'_>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) {
        if let Some(entity) = binding.entity_at(row) {
            Self::mark_changed(world, entity);
        }
    }

    /// Whether this sealed query shape has a typed contiguous projection.
    fn supports_contiguous_segments() -> bool {
        false
    }

    /// Safety: the caller must uphold the access guarantees described by `Self::append_access`.
    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>>;

    /// Safety: `binding` must have been projected from `world` for this
    /// query shape, and `row` must be within the binding's validated row range.
    unsafe fn fetch_archetype_row<'w>(
        world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        let entity = binding.entity_at(row)?;
        unsafe { Self::fetch(world, entity) }
    }
}

mod sealed {
    pub trait QuerySpecSealed {}
    pub trait QueryReadOnlySealed {}
    pub trait QueryWorldSourceSealed {}
}

impl<T> sealed::QuerySpecSealed for T where T: QueryData {}

/// Framework-owned low-level query implementation contract.
///
/// This trait is public only because it appears in public generic bounds. It is
/// sealed: downstream code cannot implement it. Safe query authoring uses the
/// framework-provided component/reference/tuple/optional/entity forms.
#[doc(hidden)]
pub trait QuerySpec: sealed::QuerySpecSealed {
    type Item<'w>;

    #[doc(hidden)]
    fn query_types() -> Vec<TypeId>;

    #[doc(hidden)]
    fn append_access(access: &mut QueryAccess);

    #[doc(hidden)]
    fn mark_changed(world: QueryCapability<'_>, entity: Entity);

    #[doc(hidden)]
    fn supports_serial_archetype_spans() -> bool;

    #[doc(hidden)]
    fn mark_changed_archetype_row(
        world: QueryCapability<'_>,
        binding: &QueryArchetypeBinding,
        row: usize,
    );

    #[doc(hidden)]
    fn supports_contiguous_segments() -> bool;

    /// # Safety
    /// The caller must uphold the access guarantees described by `Self::append_access`.
    #[doc(hidden)]
    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>>;

    /// # Safety
    /// `binding` must have been projected from `world` for this query shape,
    /// and `row` must be within its validated row range.
    #[doc(hidden)]
    unsafe fn fetch_archetype_row<'w>(
        world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>>;
}

/// Framework-controlled query-shape proof used by transferable system
/// parameters. It is crate-private so downstream code cannot add a safe
/// transfer claim for a custom query implementation.
///
/// # Safety
///
/// Implementations must encode the exact `Send`/`Sync` requirements of every
/// payload access yielded by the query shape.
pub(crate) unsafe trait TransferableQueryData: QuerySpec {
    fn prepare_worker(builder: &mut WorkerWorldBuilder<'_>);
}

/// Framework-owned classification for query shapes that only yield shared
/// component references (or entity identity). This is intentionally sealed;
/// the `&World` direct-query source must not be widened by downstream code.
#[doc(hidden)]
pub trait QueryReadOnly: sealed::QueryReadOnlySealed {}

impl sealed::QueryReadOnlySealed for Entity {}
impl QueryReadOnly for Entity {}
impl<T: Component> sealed::QueryReadOnlySealed for &T {}
impl<T: Component> QueryReadOnly for &T {}
impl<T: Component> sealed::QueryReadOnlySealed for (Entity, &T) {}
impl<T: Component> QueryReadOnly for (Entity, &T) {}
impl<A: Component, B: Component> sealed::QueryReadOnlySealed for (&A, &B) {}
impl<A: Component, B: Component> QueryReadOnly for (&A, &B) {}
impl<T: Component> sealed::QueryReadOnlySealed for Option<&T> {}
impl<T: Component> QueryReadOnly for Option<&T> {}
impl<A: Component, B: Component> sealed::QueryReadOnlySealed for (&A, Option<&B>) {}
impl<A: Component, B: Component> QueryReadOnly for (&A, Option<&B>) {}
impl<T: Component> sealed::QueryReadOnlySealed for (Entity, Option<&T>) {}
impl<T: Component> QueryReadOnly for (Entity, Option<&T>) {}
impl<A: Component, B: Component, C: Component> sealed::QueryReadOnlySealed for (&A, &B, &C) {}
impl<A: Component, B: Component, C: Component> QueryReadOnly for (&A, &B, &C) {}

/// Direct query callers are converted at the public boundary into the same
/// narrow query capability used by system extraction.  Query execution itself
/// never reconstructs a whole `World` reference from this value.
#[doc(hidden)]
pub trait QueryWorldSource<'world, Q: ?Sized = Entity>: sealed::QueryWorldSourceSealed {
    #[doc(hidden)]
    const MUTABLE_WORLD: bool;

    fn into_query_capability(self) -> QueryCapability<'world>;
}

impl sealed::QueryWorldSourceSealed for &World {}
impl sealed::QueryWorldSourceSealed for &mut World {}

impl<'world, Q: QueryReadOnly> QueryWorldSource<'world, Q> for &'world World {
    const MUTABLE_WORLD: bool = false;

    fn into_query_capability(self) -> QueryCapability<'world> {
        self.query_capability()
    }
}

impl<'world, Q: ?Sized> QueryWorldSource<'world, Q> for &'world mut World {
    const MUTABLE_WORLD: bool = true;

    fn into_query_capability(self) -> QueryCapability<'world> {
        self.query_capability_mut()
    }
}

impl<T> QuerySpec for T
where
    T: QueryData,
{
    type Item<'w> = T::Item<'w>;

    fn query_types() -> Vec<TypeId> {
        T::query_types()
    }

    fn append_access(access: &mut QueryAccess) {
        T::append_access(access);
    }

    fn mark_changed(world: QueryCapability<'_>, entity: Entity) {
        T::mark_changed(world, entity);
    }

    fn supports_serial_archetype_spans() -> bool {
        T::supports_serial_archetype_spans()
    }

    fn mark_changed_archetype_row(
        world: QueryCapability<'_>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) {
        T::mark_changed_archetype_row(world, binding, row);
    }

    fn supports_contiguous_segments() -> bool {
        T::supports_contiguous_segments()
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        unsafe { T::fetch(world, entity) }
    }

    unsafe fn fetch_archetype_row<'w>(
        world: QueryCapability<'w>,
        binding: &QueryArchetypeBinding,
        row: usize,
    ) -> Option<Self::Item<'w>> {
        unsafe { T::fetch_archetype_row(world, binding, row) }
    }
}

#[doc(hidden)]
pub struct QueryArchetypeBinding {
    span: ContiguousArchetypeSpan,
    eligible_rows: Option<Vec<bool>>,
}

impl QueryArchetypeBinding {
    fn new(span: ContiguousArchetypeSpan) -> Self {
        Self {
            span,
            eligible_rows: None,
        }
    }

    fn len(&self) -> usize {
        self.span.row_count()
    }

    fn set_eligible_rows(&mut self, eligible_rows: Vec<bool>) {
        assert_eq!(
            eligible_rows.len(),
            self.len(),
            "query archetype filter mask must match projected row count"
        );
        self.eligible_rows = Some(eligible_rows);
    }

    fn row_is_eligible(&self, row: usize) -> bool {
        self.eligible_rows
            .as_ref()
            .is_none_or(|eligible| eligible.get(row).copied().unwrap_or(false))
    }

    pub(crate) fn entity_at(&self, row: usize) -> Option<Entity> {
        self.span.entity_at(row)
    }

    pub(crate) fn component_ptr_at<T: Component>(
        &self,
        component_index: usize,
        row: usize,
    ) -> Option<*const T> {
        self.span.component_ptr_at::<T>(component_index, row)
    }

    /// # Safety
    /// The sealed query shape must own exclusive access to this projected
    /// component column for the complete query invocation.
    pub(crate) unsafe fn component_mut_ptr_at<T: Component>(
        &self,
        component_index: usize,
        row: usize,
    ) -> Option<*mut T> {
        unsafe { self.span.component_mut_ptr_at::<T>(component_index, row) }
    }

    pub(crate) fn changed_tick_ptr_at(
        &self,
        component_index: usize,
        row: usize,
    ) -> Option<NonNull<ChangeCursor>> {
        self.span.changed_tick_ptr_at(component_index, row)
    }
}

pub struct QueryState<Q, F = ()> {
    world_scope: Cell<Option<WorldScopeId>>,
    query_types: Vec<TypeId>,
    mutable_types: Vec<TypeId>,
    required_present: Vec<TypeId>,
    excluded: Vec<TypeId>,
    access: QueryAccess,
    last_run_tick: Cell<Option<ChangeCursor>>,
    scratch_pool: RefCell<Vec<Vec<Entity>>>,
    serial_archetype_spans_enabled: bool,
    _marker: PhantomData<fn() -> (Q, F)>,
}

impl<Q: QuerySpec, F: QueryFilter> QueryState<Q, F> {
    /// Creates reusable query state bound initially to `world`.
    ///
    /// # Panics
    ///
    /// Panics when the statically selected query shape contains conflicting borrows.
    /// The panic includes the conflicting access domain and target type.
    pub fn new(world: &World) -> Self {
        Self::try_new(world).unwrap_or_else(|error| panic!("invalid query state: {error}"))
    }

    pub(crate) fn try_new(world: &World) -> Result<Self, QueryError> {
        let state = Self::bound(world.scope_id());
        if let Some(conflict) = state.access.borrow_conflict() {
            return Err(QueryError::ConflictingBorrow {
                domain: conflict.domain(),
                target: conflict.name(),
            });
        }
        Ok(state)
    }

    pub fn access(&self) -> &QueryAccess {
        &self.access
    }

    /// Fallibly expose one typed, row-aligned segment per matching archetype.
    ///
    /// The returned borrow excludes structural mutation of `world` until all
    /// segments are dropped. Segment order is unspecified; this API is only for
    /// direct serial queries and never falls back to scalar iteration. For a
    /// mutable query, change tracking and applicable index invalidation for a
    /// segment are completed immediately before its first mutable slice is
    /// exposed; rows in segments never accessed mutably remain unchanged.
    pub fn try_contiguous_segments<'w, W>(
        &self,
        world: W,
    ) -> Result<ContiguousSegments<'w>, ContiguousQueryError>
    where
        Q: 'w,
        F: 'w,
        W: QueryWorldSource<'w, Q>,
    {
        if !Q::supports_contiguous_segments() {
            return Err(ContiguousQueryError::UnsupportedQueryShape);
        }
        if !F::supports_contiguous_segments() {
            return Err(ContiguousQueryError::UnsupportedFilterShape);
        }

        let component_types = Q::query_types();
        for (index, component_type) in component_types.iter().enumerate() {
            if component_types[index + 1..].contains(component_type) {
                return Err(ContiguousQueryError::AliasedComponentType);
            }
        }

        let mutable_types: Vec<_> = self
            .access
            .component_writes()
            .iter()
            .map(|access| access.type_id())
            .collect();
        if !W::MUTABLE_WORLD && !mutable_types.is_empty() {
            return Err(ContiguousQueryError::MutableWorldRequired);
        }
        let world = world.into_query_capability();
        self.rebind_world_scope(world.world_scope());
        let segments = ContiguousSegments::new(
            world,
            &self.required_present,
            &self.excluded,
            &component_types,
            &mutable_types,
        )?;
        self.last_run_tick.set(Some(world.current_change_tick()));
        Ok(segments)
    }

    pub fn with<T: Component>(mut self) -> Self {
        push_unique_type(&mut self.required_present, TypeId::of::<T>());
        self
    }

    pub fn without<T: Component>(mut self) -> Self {
        push_unique_type(&mut self.excluded, TypeId::of::<T>());
        self
    }

    pub fn iter<'w, W>(&'w self, world: W) -> impl Iterator<Item = Q::Item<'w>> + 'w
    where
        Q: 'w,
        F: 'w,
        W: QueryWorldSource<'w, Q>,
    {
        let iter: QueryIter<'w, 'w, Q, F> = self.iter_capability(world.into_query_capability());
        iter
    }

    /// Returns the item for `entity` when it satisfies the complete query predicate.
    ///
    /// `None` means that this entity yields no item for this query. The result does
    /// not distinguish among a non-live/foreign/stale entity, missing required query
    /// components, exclusion constraints, or a typed/stateful filter rejecting the
    /// entity. Callers that need entity or component validity diagnostics should use
    /// the owning `World` entity/component APIs before query membership lookup.
    pub fn get<'w, W>(&self, world: W, entity: Entity) -> Option<Q::Item<'w>>
    where
        Q: 'w,
        W: QueryWorldSource<'w, Q>,
    {
        self.get_capability(world.into_query_capability(), entity)
    }

    pub fn single<'w, W>(&self, world: W) -> Result<Q::Item<'w>, QueryError>
    where
        Q: 'w,
        W: QueryWorldSource<'w, Q>,
    {
        self.single_capability(world.into_query_capability())
    }

    fn iter_capability<'w, 'state>(
        &'state self,
        world: QueryCapability<'w>,
    ) -> QueryIter<'w, 'state, Q, F>
    where
        Q: 'w,
    {
        self.rebind_world_scope(world.world_scope());
        let since_tick = self
            .last_run_tick
            .get()
            .expect("query state must be bound before iteration");

        if self.serial_archetype_spans_enabled
            && let Some(spans) = world.collect_serial_query_spans(
                &self.required_present,
                &self.excluded,
                &self.query_types,
                &self.mutable_types,
            )
        {
            let mut bindings = spans
                .into_iter()
                .map(QueryArchetypeBinding::new)
                .collect::<Vec<_>>();
            if F::needs_tick_filter() {
                for binding in &mut bindings {
                    let eligible_rows = (0..binding.len())
                        .map(|row| {
                            let entity = binding
                                .entity_at(row)
                                .expect("validated query archetype row must contain an entity");
                            F::matches_entity(world, entity, since_tick)
                        })
                        .collect();
                    binding.set_eligible_rows(eligible_rows);
                }
            }
            self.last_run_tick.set(Some(world.current_change_tick()));
            return QueryIter {
                world,
                archetype_bindings: Some(bindings),
                entities: None,
                scratch_pool: &self.scratch_pool,
                mark_changes: !self.mutable_types.is_empty(),
                binding_index: 0,
                binding_row: 0,
                index: 0,
                _marker: PhantomData,
            };
        }

        let mut entities = self.acquire_scratch_vec();
        // Fallback path for query forms or worker capabilities that do not
        // support direct serial archetype/span iteration.
        self.matching_entities_into(world, &mut entities);
        self.last_run_tick.set(Some(world.current_change_tick()));
        QueryIter {
            world,
            archetype_bindings: None,
            entities: Some(entities),
            scratch_pool: &self.scratch_pool,
            mark_changes: !self.mutable_types.is_empty(),
            binding_index: 0,
            binding_row: 0,
            index: 0,
            _marker: PhantomData,
        }
    }

    fn get_capability<'w>(&self, world: QueryCapability<'w>, entity: Entity) -> Option<Q::Item<'w>>
    where
        Q: 'w,
    {
        self.rebind_world_scope(world.world_scope());
        let matches = self.matches_entity(world, entity);
        self.last_run_tick.set(Some(world.current_change_tick()));
        if !matches {
            return None;
        }
        Q::mark_changed(world, entity);
        // Safety: query borrow conflicts were rejected when this QueryState was created.
        unsafe { Q::fetch(world, entity) }
    }

    fn single_capability<'w>(&self, world: QueryCapability<'w>) -> Result<Q::Item<'w>, QueryError>
    where
        Q: 'w,
    {
        self.rebind_world_scope(world.world_scope());
        let mut entities = self.acquire_scratch_vec();
        self.matching_entities_into(world, &mut entities);
        self.last_run_tick.set(Some(world.current_change_tick()));
        if entities.is_empty() {
            self.release_scratch_vec(entities);
            return Err(QueryError::NoResults);
        }
        if entities.len() > 1 {
            let count = entities.len();
            self.release_scratch_vec(entities);
            return Err(QueryError::MultipleResults { count });
        }
        Q::mark_changed(world, entities[0]);
        // Safety: exactly one matching entity exists and query borrow conflicts
        // were rejected when this QueryState was created.
        let result = unsafe { Q::fetch(world, entities[0]) }.ok_or(QueryError::NoResults);
        self.release_scratch_vec(entities);
        result
    }

    pub(crate) fn unbound() -> Result<Self, QueryError> {
        let state = Self::new_state(None);
        if let Some(conflict) = state.access.borrow_conflict() {
            return Err(QueryError::ConflictingBorrow {
                domain: conflict.domain(),
                target: conflict.name(),
            });
        }
        Ok(state)
    }

    fn bound(world_scope: WorldScopeId) -> Self {
        Self::new_state(Some(world_scope))
    }

    fn new_state(world_scope: Option<WorldScopeId>) -> Self {
        let query_types = Q::query_types();
        let mut required = Vec::new();
        let mut excluded = Vec::new();
        F::configure(&mut required, &mut excluded);
        let mut required_present = query_types.clone();
        for type_id in &required {
            push_unique_type(&mut required_present, *type_id);
        }

        let mut access = QueryAccess::default();
        Q::append_access(&mut access);
        let query_borrow_checkpoint = access.borrow_checkpoint();
        F::append_access(&mut access);
        // Filter callbacks only inspect a shared World and do not manufacture
        // query-item references. Keep their scheduler metadata but exclude it
        // from the alias proof captured from Q itself.
        access.restore_borrow_checkpoint(query_borrow_checkpoint);

        let mutable_types = access
            .component_writes()
            .iter()
            .map(|component| component.type_id())
            .collect();

        Self {
            world_scope: Cell::new(world_scope),
            query_types,
            mutable_types,
            required_present,
            excluded,
            access,
            last_run_tick: Cell::new(world_scope.map(ChangeCursor::origin)),
            scratch_pool: RefCell::new(Vec::new()),
            serial_archetype_spans_enabled: Q::supports_serial_archetype_spans(),
            _marker: PhantomData,
        }
    }

    fn rebind_world_scope(&self, actual: WorldScopeId) {
        if self.world_scope.get() == Some(actual) {
            return;
        }

        self.world_scope.set(Some(actual));
        self.last_run_tick.set(Some(ChangeCursor::origin(actual)));
    }

    fn matching_entities_into(&self, world: QueryCapability<'_>, out: &mut Vec<Entity>) {
        let since_tick = self
            .last_run_tick
            .get()
            .expect("query state must be bound before matching entities");
        world.matching_entities_into(&self.required_present, &self.excluded, out);
        if F::needs_tick_filter() {
            out.retain(|entity| F::matches_entity(world, *entity, since_tick));
        }
    }

    fn matches_entity(&self, world: QueryCapability<'_>, entity: Entity) -> bool {
        let since_tick = self
            .last_run_tick
            .get()
            .expect("query state must be bound before matching an entity");
        world.entity_matches_component_constraints(entity, &self.required_present, &self.excluded)
            && (!F::needs_tick_filter() || F::matches_entity(world, entity, since_tick))
    }

    fn acquire_scratch_vec(&self) -> Vec<Entity> {
        self.scratch_pool.borrow_mut().pop().unwrap_or_default()
    }

    fn release_scratch_vec(&self, mut entities: Vec<Entity>) {
        entities.clear();
        let mut pool = self.scratch_pool.borrow_mut();
        if pool.len() < 4 {
            pool.push(entities);
        }
    }
}

type QueryIterMarker<'w, 'state, Q, F> =
    (&'state QueryState<Q, F>, fn() -> <Q as QuerySpec>::Item<'w>);

pub struct Query<'world, 'state, Q, F = ()> {
    world: QueryCapability<'world>,
    state: NonNull<QueryState<Q, F>>,
    _marker: PhantomData<&'state mut QueryState<Q, F>>,
}

impl<'world, 'state, Q, F> Query<'world, 'state, Q, F> {
    pub(crate) fn new(world: QueryCapability<'world>, state: &'state mut QueryState<Q, F>) -> Self {
        Self {
            world,
            state: NonNull::from(state),
            _marker: PhantomData,
        }
    }

    fn capability<'query>(&'query self) -> QueryCapability<'query> {
        self.world
    }
}

impl<'world, 'state, Q: QuerySpec, F: QueryFilter> Query<'world, 'state, Q, F> {
    pub fn access(&self) -> &QueryAccess {
        unsafe { self.state.as_ref().access() }
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Q::Item<'_>> + '_ {
        // Safety: system execution guarantees the world pointer remains valid for this call.
        let iter: QueryIter<'_, 'state, Q, F> =
            unsafe { self.state.as_ref().iter_capability(self.capability()) };
        iter
    }

    /// Returns the item for `entity` when it satisfies this system query's complete predicate.
    ///
    /// `None` is query non-membership and intentionally does not distinguish whether
    /// the entity is non-live, lacks required components, or is rejected by a filter.
    pub fn get(&mut self, entity: Entity) -> Option<Q::Item<'_>> {
        // Safety: system execution guarantees the world pointer remains valid for this call.
        unsafe {
            self.state
                .as_ref()
                .get_capability(self.capability(), entity)
        }
    }

    pub fn single(&mut self) -> Result<Q::Item<'_>, QueryError> {
        // Safety: system execution guarantees the world pointer remains valid for this call.
        unsafe { self.state.as_ref().single_capability(self.capability()) }
    }
}

struct QueryIter<'w, 'state, Q: QuerySpec, F> {
    world: QueryCapability<'w>,
    archetype_bindings: Option<Vec<QueryArchetypeBinding>>,
    entities: Option<Vec<Entity>>,
    scratch_pool: &'state RefCell<Vec<Vec<Entity>>>,
    mark_changes: bool,
    binding_index: usize,
    binding_row: usize,
    index: usize,
    _marker: PhantomData<QueryIterMarker<'w, 'state, Q, F>>,
}

impl<'w, 'state, Q: QuerySpec, F: QueryFilter> Iterator for QueryIter<'w, 'state, Q, F> {
    type Item = Q::Item<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(bindings) = self.archetype_bindings.as_ref() {
            loop {
                let binding = bindings.get(self.binding_index)?;
                if self.binding_row >= binding.len() {
                    self.binding_index += 1;
                    self.binding_row = 0;
                    continue;
                }

                let row = self.binding_row;
                self.binding_row += 1;
                if !binding.row_is_eligible(row) {
                    continue;
                }

                if self.mark_changes {
                    Q::mark_changed_archetype_row(self.world, binding, row);
                }
                // Safety: the binding was freshly projected from this serial
                // query capability for Q's exact sealed access shape. Row is
                // within the validated archetype range and any per-entity
                // filter was evaluated before mutable exposure.
                return Some(
                    unsafe { Q::fetch_archetype_row(self.world, binding, row) }.expect(
                        "validated query archetype row must contain every projected component",
                    ),
                );
            }
        }

        let entities = self.entities.as_ref()?;
        let entities_ptr = entities.as_ptr();
        let entities_len = entities.len();

        while self.index < entities_len {
            // Safety: `self.index < entities_len` and `entities_ptr` points to `entities`.
            let entity = unsafe { *entities_ptr.add(self.index) };
            self.index += 1;
            if self.mark_changes {
                Q::mark_changed(self.world, entity);
            }
            // Safety: QueryState validated aliasing before constructing this iterator,
            // which holds the invocation-scoped query capability contract.
            if let Some(item) = unsafe { Q::fetch(self.world, entity) } {
                return Some(item);
            }
        }
        None
    }
}

impl<'w, 'state, Q: QuerySpec, F> Drop for QueryIter<'w, 'state, Q, F> {
    fn drop(&mut self) {
        if let Some(mut entities) = self.entities.take() {
            entities.clear();
            let mut pool = self.scratch_pool.borrow_mut();
            if pool.len() < 4 {
                pool.push(entities);
            }
        }
    }
}

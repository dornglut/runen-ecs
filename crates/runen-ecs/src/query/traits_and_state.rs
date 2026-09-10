// Owner: RunenECS - Query Runtime
use super::access_and_filters::{QueryAccess, QueryFilter, push_unique_type};
use crate::component::Component;
use crate::entity::{Entity, WorldScopeId};
use crate::errors::QueryError;
use crate::storage::ArchetypeExecutionBinding;
use crate::world::{ChangeCursor, QueryCapability, World};
use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

pub trait QueryData {
    type Item<'w>;

    fn query_types() -> Vec<TypeId>;
    fn append_access(access: &mut QueryAccess);

    fn mark_changed(_world: QueryCapability<'_>, _entity: Entity) {}

    /// Enables cached mark/fetch hooks that avoid per-entity setup work inside the iterator loop.
    fn supports_fast_path() -> bool {
        false
    }

    fn prepare_fast_cache(_world: QueryCapability<'_>, _cache: &mut QueryFastCache) -> bool {
        false
    }

    fn mark_changed_fast(world: QueryCapability<'_>, entity: Entity, _cache: &mut QueryFastCache) {
        Self::mark_changed(world, entity);
    }

    /// Enables archetype-row execution instead of the entity-list fallback path.
    fn supports_archetype_execution() -> bool {
        false
    }

    fn collect_archetype_rows(
        _world: QueryCapability<'_>,
        _required_present: &[TypeId],
        _excluded: &[TypeId],
        _rows: &mut Vec<QueryArchetypeRow>,
        _cache: &mut QueryFastCache,
    ) -> bool {
        false
    }

    /// Safety: the caller must uphold the access guarantees described by `Self::append_access`.
    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>>;

    /// Safety: the caller must uphold the access guarantees described by `Self::append_access`.
    unsafe fn fetch_fast<'w>(
        world: QueryCapability<'w>,
        entity: Entity,
        _cache: &mut QueryFastCache,
    ) -> Option<Self::Item<'w>> {
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
    fn supports_fast_path() -> bool;

    #[doc(hidden)]
    fn prepare_fast_cache(world: QueryCapability<'_>, cache: &mut QueryFastCache) -> bool;

    #[doc(hidden)]
    fn mark_changed_fast(world: QueryCapability<'_>, entity: Entity, cache: &mut QueryFastCache);

    #[doc(hidden)]
    fn supports_archetype_execution() -> bool;

    #[doc(hidden)]
    fn collect_archetype_rows(
        world: QueryCapability<'_>,
        required_present: &[TypeId],
        excluded: &[TypeId],
        rows: &mut Vec<QueryArchetypeRow>,
        cache: &mut QueryFastCache,
    ) -> bool;

    /// # Safety
    /// The caller must uphold the access guarantees described by `Self::append_access`.
    #[doc(hidden)]
    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>>;

    /// # Safety
    /// The caller must uphold the access guarantees described by `Self::append_access`.
    #[doc(hidden)]
    unsafe fn fetch_fast<'w>(
        world: QueryCapability<'w>,
        entity: Entity,
        cache: &mut QueryFastCache,
    ) -> Option<Self::Item<'w>>;
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
    fn into_query_capability(self) -> QueryCapability<'world>;
}

impl sealed::QueryWorldSourceSealed for &World {}
impl sealed::QueryWorldSourceSealed for &mut World {}

impl<'world, Q: QueryReadOnly> QueryWorldSource<'world, Q> for &'world World {
    fn into_query_capability(self) -> QueryCapability<'world> {
        self.query_capability()
    }
}

impl<'world, Q: ?Sized> QueryWorldSource<'world, Q> for &'world mut World {
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

    fn supports_fast_path() -> bool {
        T::supports_fast_path()
    }

    fn prepare_fast_cache(world: QueryCapability<'_>, cache: &mut QueryFastCache) -> bool {
        T::prepare_fast_cache(world, cache)
    }

    fn mark_changed_fast(world: QueryCapability<'_>, entity: Entity, cache: &mut QueryFastCache) {
        T::mark_changed_fast(world, entity, cache);
    }

    fn supports_archetype_execution() -> bool {
        T::supports_archetype_execution()
    }

    fn collect_archetype_rows(
        world: QueryCapability<'_>,
        required_present: &[TypeId],
        excluded: &[TypeId],
        rows: &mut Vec<QueryArchetypeRow>,
        cache: &mut QueryFastCache,
    ) -> bool {
        T::collect_archetype_rows(world, required_present, excluded, rows, cache)
    }

    unsafe fn fetch<'w>(world: QueryCapability<'w>, entity: Entity) -> Option<Self::Item<'w>> {
        unsafe { T::fetch(world, entity) }
    }

    unsafe fn fetch_fast<'w>(
        world: QueryCapability<'w>,
        entity: Entity,
        cache: &mut QueryFastCache,
    ) -> Option<Self::Item<'w>> {
        unsafe { T::fetch_fast(world, entity, cache) }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct QueryArchetypeRow {
    pub entity: Entity,
    pub archetype_index: usize,
    pub row: usize,
}

#[derive(Debug, Clone, Default)]
pub struct QueryFastCache {
    // QueryState semantic ownership is bound separately to C1's opaque world
    // identity. This field is only a cache invalidation marker.
    pub(crate) world_scope: Option<WorldScopeId>,
    // Reused archetype bindings for archetype-row execution forms.
    pub(crate) archetype_bindings: Vec<ArchetypeExecutionBinding>,
}

pub struct QueryState<Q, F = ()> {
    world_scope: Cell<WorldScopeId>,
    required_present: Vec<TypeId>,
    excluded: Vec<TypeId>,
    access: QueryAccess,
    last_run_tick: Cell<ChangeCursor>,
    scratch_pool: Rc<RefCell<Vec<Vec<Entity>>>>,
    archetype_row_scratch_pool: Rc<RefCell<Vec<Vec<QueryArchetypeRow>>>>,
    fast_fetch_enabled: bool,
    archetype_execution_enabled: bool,
    fast_cache: RefCell<QueryFastCache>,
    _marker: PhantomData<(Q, F)>,
}

impl<Q: QuerySpec, F: QueryFilter> QueryState<Q, F> {
    pub fn new(world: &World) -> Self {
        Self::try_new(world).unwrap_or_else(|error| panic!("invalid query state: {error}"))
    }

    pub(crate) fn try_new(world: &World) -> Result<Self, QueryError> {
        let state = Self::detached(world.scope_id());
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

    pub fn with<T: Component>(mut self) -> Self {
        push_unique_type(&mut self.required_present, TypeId::of::<T>());
        self
    }

    pub fn without<T: Component>(mut self) -> Self {
        push_unique_type(&mut self.excluded, TypeId::of::<T>());
        self
    }

    pub fn iter<'w, W>(&self, world: W) -> impl Iterator<Item = Q::Item<'w>> + 'w
    where
        Q: 'w,
        F: 'w,
        W: QueryWorldSource<'w, Q>,
    {
        let iter: QueryIter<'w, 'w, Q, F> = self.iter_capability(world.into_query_capability());
        iter
    }

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

    fn iter_capability<'w, 'state>(&self, world: QueryCapability<'w>) -> QueryIter<'w, 'state, Q, F>
    where
        Q: 'w,
    {
        self.rebind_world_scope(world.world_scope());
        let since_tick = self.last_run_tick.get();
        let (use_fast_fetch, mut fast_cache) = self.prepare_fast_fetch(world);

        if self.archetype_execution_enabled {
            let mut rows = self.acquire_archetype_row_vec();
            if Q::collect_archetype_rows(
                world,
                &self.required_present,
                &self.excluded,
                &mut rows,
                &mut fast_cache,
            ) {
                if F::needs_tick_filter() {
                    rows.retain(|row| F::matches_entity(world, row.entity, since_tick));
                }
                self.last_run_tick.set(world.current_change_tick());
                return QueryIter {
                    world,
                    entities: None,
                    archetype_rows: Some(rows),
                    scratch_pool: Rc::clone(&self.scratch_pool),
                    archetype_row_scratch_pool: Rc::clone(&self.archetype_row_scratch_pool),
                    use_fast_fetch,
                    fast_cache,
                    index: 0,
                    _marker: PhantomData,
                };
            }
            self.release_archetype_row_vec(rows);
        }

        let mut entities = self.acquire_scratch_vec();
        // Fallback path for query forms that do not support archetype-row execution.
        self.matching_entities_into(world, &mut entities);
        self.last_run_tick.set(world.current_change_tick());
        QueryIter {
            world,
            entities: Some(entities),
            archetype_rows: None,
            scratch_pool: Rc::clone(&self.scratch_pool),
            archetype_row_scratch_pool: Rc::clone(&self.archetype_row_scratch_pool),
            use_fast_fetch,
            fast_cache,
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
        self.last_run_tick.set(world.current_change_tick());
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
        self.last_run_tick.set(world.current_change_tick());
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

    pub(crate) fn detached(world_scope: WorldScopeId) -> Self {
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

        Self {
            world_scope: Cell::new(world_scope),
            required_present,
            excluded,
            access,
            last_run_tick: Cell::new(ChangeCursor::default()),
            scratch_pool: Rc::new(RefCell::new(Vec::new())),
            archetype_row_scratch_pool: Rc::new(RefCell::new(Vec::new())),
            fast_fetch_enabled: Q::supports_fast_path(),
            archetype_execution_enabled: Q::supports_archetype_execution(),
            fast_cache: RefCell::new(QueryFastCache::default()),
            _marker: PhantomData,
        }
    }

    fn rebind_world_scope(&self, actual: WorldScopeId) {
        if self.world_scope.get() == actual {
            return;
        }

        self.world_scope.set(actual);
        self.last_run_tick.set(ChangeCursor::default());
        *self.fast_cache.borrow_mut() = QueryFastCache::default();
    }

    fn prepare_fast_fetch(&self, world: QueryCapability<'_>) -> (bool, QueryFastCache) {
        if !self.fast_fetch_enabled {
            return (false, QueryFastCache::default());
        }

        let mut cache = self.fast_cache.borrow_mut();
        let prepared = Q::prepare_fast_cache(world, &mut cache);
        (prepared, cache.clone())
    }

    fn matching_entities_into(&self, world: QueryCapability<'_>, out: &mut Vec<Entity>) {
        let since_tick = self.last_run_tick.get();
        world.matching_entities_into(&self.required_present, &self.excluded, out);
        if F::needs_tick_filter() {
            out.retain(|entity| F::matches_entity(world, *entity, since_tick));
        }
    }

    fn matches_entity(&self, world: QueryCapability<'_>, entity: Entity) -> bool {
        let since_tick = self.last_run_tick.get();
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

    fn acquire_archetype_row_vec(&self) -> Vec<QueryArchetypeRow> {
        self.archetype_row_scratch_pool
            .borrow_mut()
            .pop()
            .unwrap_or_default()
    }

    fn release_archetype_row_vec(&self, mut rows: Vec<QueryArchetypeRow>) {
        rows.clear();
        let mut pool = self.archetype_row_scratch_pool.borrow_mut();
        if pool.len() < 4 {
            pool.push(rows);
        }
    }
}

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
    entities: Option<Vec<Entity>>,
    archetype_rows: Option<Vec<QueryArchetypeRow>>,
    scratch_pool: Rc<RefCell<Vec<Vec<Entity>>>>,
    archetype_row_scratch_pool: Rc<RefCell<Vec<Vec<QueryArchetypeRow>>>>,
    use_fast_fetch: bool,
    fast_cache: QueryFastCache,
    index: usize,
    _marker: PhantomData<(&'state mut QueryState<Q, F>, Q::Item<'w>)>,
}

impl<'w, 'state, Q: QuerySpec, F> QueryIter<'w, 'state, Q, F> {
    fn mark_and_fetch(&mut self, entity: Entity) -> Option<Q::Item<'w>> {
        if self.use_fast_fetch {
            Q::mark_changed_fast(self.world, entity, &mut self.fast_cache);
            // Safety: QueryState validated aliasing before constructing this iterator,
            // which holds the invocation-scoped query capability contract.
            return unsafe { Q::fetch_fast(self.world, entity, &mut self.fast_cache) };
        }

        Q::mark_changed(self.world, entity);
        // Safety: QueryState validated aliasing before constructing this iterator,
        // which holds the invocation-scoped query capability contract.
        unsafe { Q::fetch(self.world, entity) }
    }
}

impl<'w, 'state, Q: QuerySpec, F> Iterator for QueryIter<'w, 'state, Q, F> {
    type Item = Q::Item<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(rows) = self.archetype_rows.as_ref() {
            let rows_ptr = rows.as_ptr();
            let rows_len = rows.len();

            while self.index < rows_len {
                // Safety: `self.index < rows_len` and `rows_ptr` points to `rows`.
                let row = unsafe { *rows_ptr.add(self.index) };
                self.index += 1;
                if let Some(item) = self.mark_and_fetch(row.entity) {
                    return Some(item);
                }
            }
            return None;
        }

        let entities = self.entities.as_ref()?;
        let entities_ptr = entities.as_ptr();
        let entities_len = entities.len();

        while self.index < entities_len {
            // Safety: `self.index < entities_len` and `entities_ptr` points to `entities`.
            let entity = unsafe { *entities_ptr.add(self.index) };
            self.index += 1;
            if let Some(item) = self.mark_and_fetch(entity) {
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

        if let Some(mut rows) = self.archetype_rows.take() {
            rows.clear();
            let mut pool = self.archetype_row_scratch_pool.borrow_mut();
            if pool.len() < 4 {
                pool.push(rows);
            }
        }
    }
}

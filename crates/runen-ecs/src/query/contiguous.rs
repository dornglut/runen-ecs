// Owner: RunenECS - Query Runtime
//! Sealed, direct-World projections over archetype-owned typed Vec columns.
//!
//! The registry preflights entity, payload, and change-metadata row counts
//! while structural mutation is excluded. Raw allocation bases stay private
//! and are converted to slices only by these lifetime-bound wrappers. Mutable
//! bookkeeping uses preflighted row metadata pointers, so advancing to another
//! segment never reborrows the archetype registry while a prior payload slice
//! may still be live. Worker capabilities do not own Vec columns and are not
//! admitted here.
use crate::component::Component;
use crate::entity::Entity;
use crate::errors::ContiguousQueryError;
use crate::world::{ChangeCursor, QueryCapability};
use std::any::TypeId;
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::slice;

#[derive(Debug, Copy, Clone)]
struct ComponentSpan {
    component_type: TypeId,
    values: NonNull<()>,
    row_count: usize,
    mutable: bool,
}

#[derive(Debug)]
struct SegmentBinding {
    entities: NonNull<Entity>,
    row_count: usize,
    components: Vec<ComponentSpan>,
    pending_changes: Vec<PendingComponentChanges>,
}

#[derive(Debug)]
struct PendingComponentChanges {
    component_type: TypeId,
    changed_ticks: Vec<NonNull<ChangeCursor>>,
}

/// The non-transferable direct-query iterator over matching archetype segments.
/// Segment order is deliberately unspecified, and no global slice is promised.
pub struct ContiguousSegments<'world> {
    world: QueryCapability<'world>,
    bindings: std::vec::IntoIter<SegmentBinding>,
    _world: PhantomData<(&'world crate::World, *mut ())>,
}

/// One row-aligned segment. Storage pointers, archetype identity, and row
/// indices remain private; safe consumers can request only typed slices for
/// components projected by the query.
pub struct ContiguousSegment<'world> {
    entities: NonNull<Entity>,
    row_count: usize,
    components: Vec<ComponentSpan>,
    pending_changes: Vec<PendingComponentChanges>,
    world: QueryCapability<'world>,
    mutations_recorded: bool,
    _world: PhantomData<(&'world crate::World, *mut ())>,
}

impl<'world> ContiguousSegments<'world> {
    pub(crate) fn new(
        world: QueryCapability<'world>,
        required_present: &[TypeId],
        excluded: &[TypeId],
        component_types: &[TypeId],
        mutable_types: &[TypeId],
    ) -> Result<Self, ContiguousQueryError> {
        let spans = world.collect_contiguous_spans(
            required_present,
            excluded,
            component_types,
            mutable_types,
        )?;
        let mut bindings = Vec::with_capacity(spans.len());

        for span in spans {
            if span.row_count == 0
                || span.components.len() != component_types.len()
                || span.components.iter().any(|component| {
                    component.row_count != span.row_count
                        || (mutable_types.contains(&component.component_type)
                            && component.changed_ticks.len() != span.row_count)
                        || (!mutable_types.contains(&component.component_type)
                            && !component.changed_ticks.is_empty())
                })
            {
                return Err(ContiguousQueryError::StorageInvariant);
            }

            let mut components = Vec::with_capacity(span.components.len());
            let mut pending_changes = Vec::new();
            for component in span.components {
                let mutable = mutable_types.contains(&component.component_type);
                if mutable {
                    pending_changes.push(PendingComponentChanges {
                        component_type: component.component_type,
                        changed_ticks: component.changed_ticks,
                    });
                }
                components.push(ComponentSpan {
                    component_type: component.component_type,
                    values: component.values,
                    row_count: component.row_count,
                    mutable,
                });
            }
            bindings.push(SegmentBinding {
                entities: span.entities,
                row_count: span.row_count,
                components,
                pending_changes,
            });
        }

        Ok(Self {
            world,
            bindings: bindings.into_iter(),
            _world: PhantomData,
        })
    }
}

impl<'world> Iterator for ContiguousSegments<'world> {
    type Item = ContiguousSegment<'world>;

    fn next(&mut self) -> Option<Self::Item> {
        self.bindings.next().map(|binding| ContiguousSegment {
            entities: binding.entities,
            row_count: binding.row_count,
            components: binding.components,
            pending_changes: binding.pending_changes,
            world: self.world,
            mutations_recorded: false,
            _world: PhantomData,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bindings.size_hint()
    }
}

impl ExactSizeIterator for ContiguousSegments<'_> {}
impl FusedIterator for ContiguousSegments<'_> {}

impl ContiguousSegment<'_> {
    /// Number of entities in this archetype-local segment.
    pub fn len(&self) -> usize {
        self.row_count
    }

    pub fn is_empty(&self) -> bool {
        self.row_count == 0
    }

    /// Entity identities aligned by row with every projected component slice.
    pub fn entities(&self) -> &[Entity] {
        // Safety: this pointer and length were checked against the non-empty
        // entity column; the World remains structurally borrowed for the
        // segment lifetime.
        unsafe { slice::from_raw_parts(self.entities.as_ptr(), self.row_count) }
    }

    /// Shared component values in the same row order as [`Self::entities`].
    pub fn component<T: Component>(&self) -> Result<&[T], ContiguousQueryError> {
        let span = self.component_span::<T>()?;
        // Safety: the component TypeId and aligned length were verified while
        // projecting this sealed query shape from its typed Vec<T> column.
        Ok(unsafe { slice::from_raw_parts(span.values.cast::<T>().as_ptr(), span.row_count) })
    }

    /// Mutably access one projected component column. Before the slice is
    /// exposed, change tracking and applicable index invalidation are recorded
    /// for every mutable query component in every row of this segment.
    pub fn component_mut<T: Component>(&mut self) -> Result<&mut [T], ContiguousQueryError> {
        let span = self.component_span::<T>()?;
        if !span.mutable {
            return Err(ContiguousQueryError::ComponentNotMutable);
        }
        self.record_mutations();
        // Safety: the query's sealed access shape grants an exclusive borrow of
        // this distinct component column. The returned borrow is tied to the
        // mutable borrow of this segment, preventing another overlapping view.
        Ok(unsafe { slice::from_raw_parts_mut(span.values.cast::<T>().as_ptr(), span.row_count) })
    }

    /// Entity identities and one mutable component column aligned by row.
    /// Both slices borrow this segment together, so callers can retain and zip
    /// them without copying entity identities. Change/index bookkeeping is
    /// recorded before the mutable component slice becomes observable.
    pub fn entity_component_mut<T: Component>(
        &mut self,
    ) -> Result<(&[Entity], &mut [T]), ContiguousQueryError> {
        let span = self.component_span::<T>()?;
        if !span.mutable {
            return Err(ContiguousQueryError::ComponentNotMutable);
        }
        self.record_mutations();
        // Safety: entity identities and the exact typed component column are
        // separate row-aligned allocations. The exclusive segment borrow
        // prevents competing views for the lifetime of both returned slices.
        Ok(unsafe {
            (
                slice::from_raw_parts(self.entities.as_ptr(), self.row_count),
                slice::from_raw_parts_mut(span.values.cast::<T>().as_ptr(), span.row_count),
            )
        })
    }

    /// Two shared columns with row-for-row alignment.
    pub fn component_pair<A: Component, B: Component>(
        &self,
    ) -> Result<(&[A], &[B]), ContiguousQueryError> {
        let (first, second) = self.component_pair_spans::<A, B>()?;
        // Safety: both pointers are checked against their exact concrete types
        // and have the same archetype row count.
        Ok(unsafe {
            (
                slice::from_raw_parts(first.values.cast::<A>().as_ptr(), first.row_count),
                slice::from_raw_parts(second.values.cast::<B>().as_ptr(), second.row_count),
            )
        })
    }

    /// A mutable first column and shared second column, row-for-row aligned.
    pub fn component_pair_mut_shared<A: Component, B: Component>(
        &mut self,
    ) -> Result<(&mut [A], &[B]), ContiguousQueryError> {
        let (first, second) = self.component_pair_spans::<A, B>()?;
        if !first.mutable {
            return Err(ContiguousQueryError::ComponentNotMutable);
        }
        self.record_mutations();
        // Safety: the query grants exclusive access to A and shared access to
        // the distinct B column; both lengths were validated during projection.
        Ok(unsafe {
            (
                slice::from_raw_parts_mut(first.values.cast::<A>().as_ptr(), first.row_count),
                slice::from_raw_parts(second.values.cast::<B>().as_ptr(), second.row_count),
            )
        })
    }

    /// A shared first column and mutable second column, row-for-row aligned.
    pub fn component_pair_shared_mut<A: Component, B: Component>(
        &mut self,
    ) -> Result<(&[A], &mut [B]), ContiguousQueryError> {
        let (first, second) = self.component_pair_spans::<A, B>()?;
        if !second.mutable {
            return Err(ContiguousQueryError::ComponentNotMutable);
        }
        self.record_mutations();
        // Safety: the query grants shared access to A and exclusive access to
        // the distinct B column; both lengths were validated during projection.
        Ok(unsafe {
            (
                slice::from_raw_parts(first.values.cast::<A>().as_ptr(), first.row_count),
                slice::from_raw_parts_mut(second.values.cast::<B>().as_ptr(), second.row_count),
            )
        })
    }

    /// Two distinct mutable columns, aligned by entity row.
    pub fn component_pair_mut<A: Component, B: Component>(
        &mut self,
    ) -> Result<(&mut [A], &mut [B]), ContiguousQueryError> {
        let (first, second) = self.component_pair_spans::<A, B>()?;
        if !first.mutable || !second.mutable {
            return Err(ContiguousQueryError::ComponentNotMutable);
        }
        self.record_mutations();
        // Safety: duplicate component types are rejected and the query's sealed
        // access proof grants exclusive access to these separate typed columns.
        Ok(unsafe {
            (
                slice::from_raw_parts_mut(first.values.cast::<A>().as_ptr(), first.row_count),
                slice::from_raw_parts_mut(second.values.cast::<B>().as_ptr(), second.row_count),
            )
        })
    }

    fn component_span<T: Component>(&self) -> Result<ComponentSpan, ContiguousQueryError> {
        self.components
            .iter()
            .find(|span| span.component_type == TypeId::of::<T>())
            .copied()
            .ok_or(ContiguousQueryError::ComponentNotProjected)
    }

    fn component_pair_spans<A: Component, B: Component>(
        &self,
    ) -> Result<(ComponentSpan, ComponentSpan), ContiguousQueryError> {
        if TypeId::of::<A>() == TypeId::of::<B>() {
            return Err(ContiguousQueryError::AliasedComponentType);
        }
        let first = self.component_span::<A>()?;
        let second = self.component_span::<B>()?;
        if first.row_count != second.row_count {
            return Err(ContiguousQueryError::StorageInvariant);
        }
        Ok((first, second))
    }

    fn record_mutations(&mut self) {
        if self.mutations_recorded {
            return;
        }

        // The preflight stored row-specific metadata pointers. Updating those
        // pointers and disjoint World change/index fields avoids reborrowing the
        // archetype registry after a slice from an earlier segment may exist.
        for row in 0..self.row_count {
            // Safety: the row was included in the matching entity column and
            // its pointer remains valid for the retained World borrow.
            let entity = unsafe { self.entities.as_ptr().add(row).read() };
            for pending in &self.pending_changes {
                let changed_tick = pending.changed_ticks[row];
                self.world.mark_contiguous_component_modified(
                    entity,
                    pending.component_type,
                    changed_tick,
                );
            }
        }
        self.mutations_recorded = true;
    }
}

// Owner: RunenECS Storage - Dense Row-Aligned Storage Primitives
use crate::entity::Entity;
use crate::world::ChangeCursor;

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct DenseRowMetadata {
    pub(crate) added_tick: ChangeCursor,
    pub(crate) changed_tick: ChangeCursor,
}

#[allow(dead_code)]
impl DenseRowMetadata {
    pub(crate) const fn new(added_tick: ChangeCursor, changed_tick: ChangeCursor) -> Self {
        Self {
            added_tick,
            changed_tick,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct DenseSwapRemove {
    pub(crate) removed_row: usize,
    pub(crate) moved_from_row: Option<usize>,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DenseColumnSwapRemove<T> {
    pub(crate) removed_value: T,
    pub(crate) removed_metadata: DenseRowMetadata,
    pub(crate) swap: DenseSwapRemove,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DenseColumnSwapRemoveBoxed<T> {
    pub(crate) removed_value: Box<T>,
    pub(crate) removed_metadata: DenseRowMetadata,
    pub(crate) swap: DenseSwapRemove,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct DenseEntitySwapRemove {
    pub(crate) removed_entity: Entity,
    pub(crate) removed_row: usize,
    pub(crate) moved_entity: Option<Entity>,
    pub(crate) moved_from_row: Option<usize>,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
pub(crate) struct DenseEntityColumn {
    entities: Vec<Entity>,
}

#[allow(dead_code)]
impl DenseEntityColumn {
    pub(crate) fn len(&self) -> usize {
        self.entities.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub(crate) fn get(&self, row: usize) -> Option<Entity> {
        self.entities.get(row).copied()
    }

    pub(crate) fn push(&mut self, entity: Entity) -> usize {
        let row = self.entities.len();
        self.entities.push(entity);
        row
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter().copied()
    }

    pub(crate) fn swap_remove(&mut self, row: usize) -> Option<DenseEntitySwapRemove> {
        if row >= self.entities.len() {
            return None;
        }

        let last_row = self.entities.len().saturating_sub(1);
        let removed_entity = self.entities.swap_remove(row);
        let moved_entity = (row != last_row).then(|| self.entities[row]);
        Some(DenseEntitySwapRemove {
            removed_entity,
            removed_row: row,
            moved_entity,
            moved_from_row: (row != last_row).then_some(last_row),
        })
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct DenseColumn<T> {
    values: Vec<Box<T>>,
    metadata: Vec<DenseRowMetadata>,
}

impl<T> Default for DenseColumn<T> {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            metadata: Vec::new(),
        }
    }
}

#[allow(dead_code)]
impl<T> DenseColumn<T> {
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub(crate) fn get(&self, row: usize) -> Option<&T> {
        self.values.get(row).map(Box::as_ref)
    }

    pub(crate) fn get_mut(&mut self, row: usize) -> Option<&mut T> {
        self.values.get_mut(row).map(Box::as_mut)
    }

    pub(crate) fn get_ptr(&self, row: usize) -> Option<*const T> {
        self.values.get(row).map(|value| value.as_ref() as *const T)
    }

    pub(crate) fn get_mut_ptr(&mut self, row: usize) -> Option<*mut T> {
        self.values
            .get_mut(row)
            .map(|value| value.as_mut() as *mut T)
    }

    pub(crate) fn metadata(&self, row: usize) -> Option<DenseRowMetadata> {
        self.metadata.get(row).copied()
    }

    pub(crate) fn push(&mut self, value: T, metadata: DenseRowMetadata) -> usize {
        self.push_boxed(Box::new(value), metadata)
    }

    pub(crate) fn push_boxed(&mut self, value: Box<T>, metadata: DenseRowMetadata) -> usize {
        let row = self.values.len();
        self.values.push(value);
        self.metadata.push(metadata);
        row
    }

    pub(crate) fn mark_changed(&mut self, row: usize, tick: ChangeCursor) -> bool {
        let Some(metadata) = self.metadata.get_mut(row) else {
            return false;
        };
        metadata.changed_tick = tick;
        true
    }

    pub(crate) fn swap_remove_boxed(
        &mut self,
        row: usize,
    ) -> Option<DenseColumnSwapRemoveBoxed<T>> {
        if row >= self.values.len() || row >= self.metadata.len() {
            return None;
        }

        let last_row = self.values.len().saturating_sub(1);
        let removed_value = self.values.swap_remove(row);
        let removed_metadata = self.metadata.swap_remove(row);
        Some(DenseColumnSwapRemoveBoxed {
            removed_value,
            removed_metadata,
            swap: DenseSwapRemove {
                removed_row: row,
                moved_from_row: (row != last_row).then_some(last_row),
            },
        })
    }

    pub(crate) fn swap_remove(&mut self, row: usize) -> Option<DenseColumnSwapRemove<T>> {
        let removed = self.swap_remove_boxed(row)?;
        Some(DenseColumnSwapRemove {
            removed_value: *removed.removed_value,
            removed_metadata: removed.removed_metadata,
            swap: removed.swap,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_column_swap_remove_reports_moved_row_and_preserves_alignment() {
        let mut column = DenseColumn::default();
        column.push(
            10_i32,
            DenseRowMetadata::new(
                ChangeCursor::from_parts(0, 1),
                ChangeCursor::from_parts(0, 1),
            ),
        );
        column.push(
            20_i32,
            DenseRowMetadata::new(
                ChangeCursor::from_parts(0, 2),
                ChangeCursor::from_parts(0, 2),
            ),
        );
        column.push(
            30_i32,
            DenseRowMetadata::new(
                ChangeCursor::from_parts(0, 3),
                ChangeCursor::from_parts(0, 3),
            ),
        );

        let removed = column.swap_remove(1).expect("row must exist");
        assert_eq!(removed.removed_value, 20);
        assert_eq!(
            removed.removed_metadata,
            DenseRowMetadata::new(
                ChangeCursor::from_parts(0, 2),
                ChangeCursor::from_parts(0, 2)
            )
        );
        assert_eq!(
            removed.swap,
            DenseSwapRemove {
                removed_row: 1,
                moved_from_row: Some(2),
            }
        );
        assert_eq!(column.len(), 2);
        assert_eq!(column.get(1), Some(&30));
        assert_eq!(
            column.metadata(1),
            Some(DenseRowMetadata::new(
                ChangeCursor::from_parts(0, 3),
                ChangeCursor::from_parts(0, 3)
            ))
        );
    }

    #[test]
    fn dense_entity_column_swap_remove_returns_moved_entity_when_needed() {
        let mut column = DenseEntityColumn::default();
        let first = Entity::test_only(1, 0);
        let second = Entity::test_only(2, 0);
        let third = Entity::test_only(3, 0);
        column.push(first);
        column.push(second);
        column.push(third);

        let removed = column.swap_remove(0).expect("row must exist");
        assert_eq!(removed.removed_entity, first);
        assert_eq!(removed.moved_entity, Some(third));
        assert_eq!(removed.moved_from_row, Some(2));
        assert_eq!(column.get(0), Some(third));
        assert_eq!(column.len(), 2);
    }
}

// Owner: RunenECS World - Change Tracking Types
use crate::entity::{Entity, WorldScopeId};
use std::any::Any;
use std::cmp::Ordering;
use std::fmt;

/// Monotonic ECS change position within one World lineage.
///
/// The private World lineage prevents a cursor from one World from being
/// interpreted as history for another. Cursors from the same World are ordered
/// by `(epoch, tick)`; cursors from different Worlds are intentionally
/// incomparable.
///
/// The explicit epoch prevents a distinct change position from aliasing when
/// the per-epoch counter reaches its boundary. The absolute two-word boundary
/// is not a recoverable runtime condition; ECS mutation paths panic before
/// reusing an exhausted position rather than silently reporting stale data.
///
/// A change cursor has no universal default because every valid cursor belongs
/// to one concrete World lineage.
///
/// ```compile_fail
/// use runen_ecs::ChangeCursor;
/// let _ = ChangeCursor::default();
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ChangeCursor {
    scope: WorldScopeId,
    epoch: u64,
    tick: u64,
}

impl ChangeCursor {
    pub(crate) const fn origin(scope: WorldScopeId) -> Self {
        Self {
            scope,
            epoch: 0,
            tick: 0,
        }
    }

    #[allow(dead_code)]
    pub(crate) const fn from_parts(scope: WorldScopeId, epoch: u64, tick: u64) -> Self {
        Self { scope, epoch, tick }
    }

    pub const fn epoch(self) -> u64 {
        self.epoch
    }

    pub const fn tick(self) -> u64 {
        self.tick
    }

    pub(crate) fn is_from(self, scope: WorldScopeId) -> bool {
        self.scope == scope
    }

    pub(crate) const fn next(self) -> Option<Self> {
        if self.tick == u64::MAX {
            match self.epoch.checked_add(1) {
                Some(epoch) => Some(Self {
                    scope: self.scope,
                    epoch,
                    tick: 0,
                }),
                None => None,
            }
        } else {
            Some(Self {
                scope: self.scope,
                epoch: self.epoch,
                tick: self.tick + 1,
            })
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum FrameworkInvariantKind {
    ChangeCursorExhausted,
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
struct FrameworkInvariantPanic {
    kind: FrameworkInvariantKind,
    message: &'static str,
}

pub(crate) fn panic_change_cursor_exhausted() -> ! {
    std::panic::panic_any(FrameworkInvariantPanic {
        kind: FrameworkInvariantKind::ChangeCursorExhausted,
        message: "ECS change cursor exhausted",
    })
}

pub(crate) fn next_change_cursor(cursor: ChangeCursor) -> ChangeCursor {
    cursor
        .next()
        .unwrap_or_else(|| panic_change_cursor_exhausted())
}

pub(crate) fn advance_change_cursor(cursor: &mut ChangeCursor) -> ChangeCursor {
    *cursor = next_change_cursor(*cursor);
    *cursor
}

#[allow(dead_code)]
pub(crate) fn framework_invariant_kind(
    payload: &(dyn Any + Send),
) -> Option<FrameworkInvariantKind> {
    payload
        .downcast_ref::<FrameworkInvariantPanic>()
        .map(|panic| panic.kind)
}

impl PartialOrd for ChangeCursor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self.scope != other.scope {
            return None;
        }
        Some((self.epoch, self.tick).cmp(&(other.epoch, other.tick)))
    }
}

impl fmt::Debug for ChangeCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChangeCursor")
            .field("epoch", &self.epoch)
            .field("tick", &self.tick)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct RemovedComponentRecord {
    pub(super) tick: ChangeCursor,
    pub(super) entity: Entity,
}

// Owner: RunenECS World - Change Tracking Types
use crate::entity::Entity;

/// Monotonic ECS change position.
///
/// The explicit epoch prevents a distinct change position from aliasing when
/// the per-epoch counter reaches its boundary. The absolute two-word boundary
/// is not a recoverable runtime condition; ECS mutation paths panic before
/// reusing an exhausted position rather than silently reporting stale data.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ChangeCursor {
    epoch: u64,
    tick: u64,
}

impl ChangeCursor {
    #[allow(dead_code)]
    pub(crate) const fn from_parts(epoch: u64, tick: u64) -> Self {
        Self { epoch, tick }
    }

    pub const fn epoch(self) -> u64 {
        self.epoch
    }

    pub const fn tick(self) -> u64 {
        self.tick
    }

    pub(crate) const fn next(self) -> Option<Self> {
        if self.tick == u64::MAX {
            match self.epoch.checked_add(1) {
                Some(epoch) => Some(Self { epoch, tick: 0 }),
                None => None,
            }
        } else {
            Some(Self {
                epoch: self.epoch,
                tick: self.tick + 1,
            })
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) struct RemovedComponentRecord {
    pub(super) tick: ChangeCursor,
    pub(super) entity: Entity,
}

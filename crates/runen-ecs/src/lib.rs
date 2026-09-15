//! RunenECS registers system declarations without borrowing a live [`World`].
//!
//! The pre-#70 World-bound registration spelling is intentionally rejected:
//!
//! ```compile_fail
//! use runen_ecs::{Runtime, ScheduleLabel, World};
//!
//! #[derive(Copy, Clone)]
//! struct Update;
//! impl ScheduleLabel for Update {}
//!
//! let mut world = World::new();
//! let mut runtime = Runtime::new();
//! runtime.add_systems::<Update, _, _>(&mut world, || {});
//! ```
//!
//! Register with `runtime.add_systems(Update, systems)?` instead.

extern crate self as runen_ecs;

mod bundle;
mod commands;
mod component;
mod entity;
mod errors;
pub mod prelude;
pub mod query;
pub mod reflect;
mod scheduler;
mod storage;
pub mod system;
mod world;
pub use bundle::Bundle;
#[doc(hidden)]
pub use bundle::{BundleComponentDescriptor, BundleComponents};
pub use commands::{BatchCommands, Commands, LocalBatchCommands, LocalCommands};
pub use component::{Component, Resource};
pub use entity::Entity;
pub use errors::{
    ChangeCursorError, CommandError, EntityAllocationError, EntityError, QueryError, ResourceError,
    RuntimeError,
};
pub use query::{
    Added, Changed, Query, QueryAccess, QueryState, QueryTypeAccess, Removed, RemovedQuery,
    RemovedState, With, Without,
};
pub use reflect::{
    EnumInfo, EnumVariantInfo, FieldInfo, Reflect, ReflectShape, ReflectValueMut, ReflectValueRef,
    StructInfo, StructValueMut, StructValueRef, TypeInfo, TypeRegistry,
};
pub use runen_ecs_macros::{Bundle, Component, Reflect, Resource, SystemParam};
pub use system::{
    ConfiguredSystem, DeferredPublicationFrontier, DeferredRecorderClass, DeferredRecorderConflict,
    ExecutionMobility, IntoSystem, IntoSystemConfigs, IntoSystemSetKey, InvokerThreadSystem,
    OrderingPresence, ParamSlotDescriptor, Res, ResMut, Runtime, ScheduleAccessAmbiguity,
    ScheduleAccessConflict, ScheduleAccessConflictKind, ScheduleAccessDomain, ScheduleInspection,
    ScheduleKey, ScheduleLabel, ScheduleOrderingCycle, ScheduleOrderingResolution,
    ScheduleOrderingResolutionKind, SchedulePairwiseConcurrencyAssessment, SchedulePrecedenceEdge,
    SchedulePrecedencePath, SchedulePrecedenceReason, SchedulePublicationFrontier,
    SchedulePublicationObligation, ScheduleValidationError, SystemConfigExt, SystemMobilityExt,
    SystemParam, SystemParamContext, SystemParamError, SystemSet, SystemSetKey,
    TransferableSystemParam, WorkerPrepareContext, WorldMut,
};
pub use world::{ChangeCursor, EntityMut, EntityRef, Mut, World};

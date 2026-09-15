// The parallel selector is a supported deterministic physical realization of the
// independent serial executor. Its failure/permutation conformance lives with the
// executor module so the semantic boundary stays visible next to the implementation.
mod parallel_executor;

use super::OrderingDirection;
use super::extract::{
    DeferredRecorderClass, DeferredRecorderConflict, SystemParam, SystemParamContext,
    SystemParamError, TransferableSystemParam, WorkerPrepareContext,
};
use crate::commands::TransferableCommandBuffer;
use crate::errors::RuntimeError;
use crate::scheduler::access::{AccessKey, SystemAccess};
use crate::scheduler::inspection::ScheduleInspection;
use crate::scheduler::label::{ScheduleKey, ScheduleLabel, SystemSet, SystemSetKey};
use crate::scheduler::plan::ScheduleRegistry;
use crate::scheduler::system::{
    OrderingDeclaration, ParamSlotDescriptor, RegisteredSystem, WorkerInvocationOutcome,
    WorkerInvocationReport, WorkerPanicPhase,
};
use crate::world::MutationJournal;
use std::cell::RefCell;
use std::error::Error;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use crate::query::QueryAccess;
use crate::{Commands, LocalCommands, World};

type Result<T> = std::result::Result<T, RuntimeError>;

type DeferredCommands = Rc<RefCell<Vec<DeferredCommandBuffer>>>;

pub(crate) enum DeferredCommandBuffer {
    Local(LocalCommands<'static>),
    Transferable(TransferableCommandBuffer),
}

pub(crate) enum InvocationOutcome {
    None,
    Local(LocalCommands<'static>),
    Transferable(TransferableCommandBuffer),
}

struct DeferredCommandsUnwindGuard {
    deferred_commands: DeferredCommands,
}

impl DeferredCommandsUnwindGuard {
    fn new(deferred_commands: DeferredCommands) -> Self {
        Self { deferred_commands }
    }
}

impl Drop for DeferredCommandsUnwindGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.deferred_commands.borrow_mut().clear();
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DeferredPublicationFrontier {
    schedule: ScheduleKey,
    ordinal: usize,
}

impl DeferredPublicationFrontier {
    pub const fn schedule(self) -> ScheduleKey {
        self.schedule
    }

    pub const fn ordinal(self) -> usize {
        self.ordinal
    }
}

pub trait SystemOutput {
    fn into_result(self) -> std::result::Result<(), Box<dyn Error + Send + Sync>>;
}

impl SystemOutput for () {
    fn into_result(self) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

impl<E> SystemOutput for std::result::Result<(), E>
where
    E: Into<Box<dyn Error + Send + Sync>> + 'static,
{
    fn into_result(self) -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
        self.map_err(Into::into)
    }
}

pub trait IntoSystem<Marker>: 'static {
    fn into_registered_system<L: ScheduleLabel>(self) -> Result<RegisteredSystem>;
}

#[derive(Debug, Clone, Default)]
struct SystemConfigMetadata {
    sets: Vec<SystemSetKey>,
    ordering_declarations: Vec<OrderingDeclaration>,
}

impl SystemConfigMetadata {
    fn with_set(&mut self, key: SystemSetKey) {
        if !self.sets.contains(&key) {
            self.sets.push(key);
        }
    }

    fn ordering(&mut self, declaration: OrderingDeclaration) {
        OrderingDeclaration::normalize_into(&mut self.ordering_declarations, declaration);
    }

    fn before_set(&mut self, key: SystemSetKey) {
        self.ordering(OrderingDeclaration::required(
            OrderingDirection::Before,
            key,
        ));
    }

    fn after_set(&mut self, key: SystemSetKey) {
        self.ordering(OrderingDeclaration::required(OrderingDirection::After, key));
    }

    fn before_if_present_set(&mut self, key: SystemSetKey) {
        self.ordering(OrderingDeclaration::optional(
            OrderingDirection::Before,
            key,
        ));
    }

    fn after_if_present_set(&mut self, key: SystemSetKey) {
        self.ordering(OrderingDeclaration::optional(OrderingDirection::After, key));
    }

    fn apply(&self, system: &mut RegisteredSystem) {
        for key in &self.sets {
            system.with_set_key(*key);
        }
        for declaration in &self.ordering_declarations {
            system.add_ordering_declaration(*declaration);
        }
    }
}

pub struct ConfiguredSystem<S, Marker> {
    system: S,
    config: SystemConfigMetadata,
    _marker: PhantomData<fn() -> Marker>,
}

/// Explicitly restricts a system to the thread invoking its schedule.
pub struct InvokerThreadSystem<S>(pub(crate) S);

pub trait SystemMobilityExt: Sized {
    fn on_invoker_thread(self) -> InvokerThreadSystem<Self> {
        InvokerThreadSystem(self)
    }
}

impl<S> SystemMobilityExt for S {}

impl<S, Marker> ConfiguredSystem<S, Marker> {
    fn new(system: S) -> Self {
        Self {
            system,
            config: SystemConfigMetadata::default(),
            _marker: PhantomData,
        }
    }

    pub fn in_set<Set>(mut self, set: Set) -> Self
    where
        Set: SystemSet,
    {
        self.config.with_set(set.key());
        self
    }

    pub fn before<Set>(mut self, set: Set) -> Self
    where
        Set: SystemSet,
    {
        self.config.before_set(set.key());
        self
    }

    pub fn after<Set>(mut self, set: Set) -> Self
    where
        Set: SystemSet,
    {
        self.config.after_set(set.key());
        self
    }

    pub fn before_if_present<Set>(mut self, set: Set) -> Self
    where
        Set: SystemSet,
    {
        self.config.before_if_present_set(set.key());
        self
    }

    pub fn after_if_present<Set>(mut self, set: Set) -> Self
    where
        Set: SystemSet,
    {
        self.config.after_if_present_set(set.key());
        self
    }
}

pub trait SystemConfigExt<Marker>: IntoSystem<Marker> + Sized {
    fn in_set<Set>(self, set: Set) -> ConfiguredSystem<Self, Marker>
    where
        Set: SystemSet,
    {
        ConfiguredSystem::new(self).in_set(set)
    }

    fn before<Set>(self, set: Set) -> ConfiguredSystem<Self, Marker>
    where
        Set: SystemSet,
    {
        ConfiguredSystem::new(self).before(set)
    }

    fn after<Set>(self, set: Set) -> ConfiguredSystem<Self, Marker>
    where
        Set: SystemSet,
    {
        ConfiguredSystem::new(self).after(set)
    }

    fn before_if_present<Set>(self, set: Set) -> ConfiguredSystem<Self, Marker>
    where
        Set: SystemSet,
    {
        ConfiguredSystem::new(self).before_if_present(set)
    }

    fn after_if_present<Set>(self, set: Set) -> ConfiguredSystem<Self, Marker>
    where
        Set: SystemSet,
    {
        ConfiguredSystem::new(self).after_if_present(set)
    }
}

impl<S, Marker> SystemConfigExt<Marker> for S where S: IntoSystem<Marker> + Sized {}

mod system_configs_sealed {
    use super::{RegisteredSystem, Result, ScheduleLabel};

    pub trait RegisterSystemConfigs<Marker> {
        fn register<L: ScheduleLabel>(self) -> Result<Vec<RegisteredSystem>>;
    }
}

pub trait IntoSystemConfigs<Marker>: system_configs_sealed::RegisterSystemConfigs<Marker> {}

impl<S, Marker> IntoSystemConfigs<Marker> for S where
    S: system_configs_sealed::RegisterSystemConfigs<Marker>
{
}

impl<S, Marker> system_configs_sealed::RegisterSystemConfigs<Marker> for S
where
    S: IntoSystem<Marker>,
{
    fn register<L: ScheduleLabel>(self) -> Result<Vec<RegisteredSystem>> {
        Ok(vec![self.into_registered_system::<L>()?])
    }
}

impl<S, Marker> IntoSystem<Marker> for ConfiguredSystem<S, Marker>
where
    S: IntoSystem<Marker>,
    Marker: 'static,
{
    fn into_registered_system<L: ScheduleLabel>(self) -> Result<RegisteredSystem> {
        let mut registered = self.system.into_registered_system::<L>()?;
        self.config.apply(&mut registered);
        Ok(registered)
    }
}

macro_rules! impl_into_system_configs_tuple {
    ($(($name:ident, $marker:ident, $index:tt)),+ $(,)?) => {
        impl<$($name, $marker,)+> system_configs_sealed::RegisterSystemConfigs<($($marker,)+)>
            for ($($name,)+)
        where
            $($name: system_configs_sealed::RegisterSystemConfigs<$marker>,)+
        {
            fn register<Sched: ScheduleLabel>(
                self,
            ) -> Result<Vec<RegisteredSystem>> {
                let mut systems = Vec::new();
                $(systems.extend(self.$index.register::<Sched>()?);)+
                Ok(systems)
            }
        }
    };
}

impl_into_system_configs_tuple!((A, AMarker, 0), (B, BMarker, 1));
impl_into_system_configs_tuple!((A, AMarker, 0), (B, BMarker, 1), (C, CMarker, 2));
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9),
    (K, KMarker, 10)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9),
    (K, KMarker, 10),
    (L, LMarker, 11)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9),
    (K, KMarker, 10),
    (L, LMarker, 11),
    (M, MMarker, 12)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9),
    (K, KMarker, 10),
    (L, LMarker, 11),
    (M, MMarker, 12),
    (N, NMarker, 13)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9),
    (K, KMarker, 10),
    (L, LMarker, 11),
    (M, MMarker, 12),
    (N, NMarker, 13),
    (O, OMarker, 14)
);
impl_into_system_configs_tuple!(
    (A, AMarker, 0),
    (B, BMarker, 1),
    (C, CMarker, 2),
    (D, DMarker, 3),
    (E, EMarker, 4),
    (F, FMarker, 5),
    (G, GMarker, 6),
    (H, HMarker, 7),
    (I, IMarker, 8),
    (J, JMarker, 9),
    (K, KMarker, 10),
    (L, LMarker, 11),
    (M, MMarker, 12),
    (N, NMarker, 13),
    (O, OMarker, 14),
    (P, PMarker, 15)
);

trait SystemParamState: Sized {
    type State: 'static;
    type Item<'world, 'state>;

    fn init_state() -> std::result::Result<Self::State, SystemParamError>;
    fn deferred_recorder_class()
    -> std::result::Result<DeferredRecorderClass, DeferredRecorderConflict>;
    fn access(state: &Self::State) -> QueryAccess;
    fn slot_descriptor() -> ParamSlotDescriptor;

    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> std::result::Result<Self::Item<'world, 'state>, SystemParamError>;
}

impl<T> SystemParamState for T
where
    T: SystemParam,
{
    type State = T::State;
    type Item<'world, 'state> = T::Item<'world, 'state>;

    fn init_state() -> std::result::Result<Self::State, SystemParamError> {
        T::init_state()
    }

    fn deferred_recorder_class()
    -> std::result::Result<DeferredRecorderClass, DeferredRecorderConflict> {
        T::deferred_recorder_class()
    }

    fn access(state: &Self::State) -> QueryAccess {
        T::access(state)
    }

    fn slot_descriptor() -> ParamSlotDescriptor {
        T::slot_descriptor()
    }

    unsafe fn extract<'world, 'state>(
        state: &'state mut Self::State,
        context: SystemParamContext<'world>,
    ) -> std::result::Result<Self::Item<'world, 'state>, SystemParamError> {
        unsafe { T::extract(state, context) }
    }
}

fn validate_borrow_access(system_name: &str, access_parts: &[QueryAccess]) -> Result<()> {
    let mut merged = QueryAccess::default();
    for access in access_parts {
        merged.extend(access.clone());
    }
    if let Some(conflict) = merged.borrow_conflict() {
        return Err(RuntimeError::Setup {
            message: format!(
                "system '{}' has conflicting param borrows: {} {}",
                system_name,
                conflict.domain(),
                conflict.name()
            ),
        });
    }
    Ok(())
}

fn merge_access(system_name: &str, access_parts: &[SystemAccess]) -> Result<SystemAccess> {
    let mut merged = SystemAccess::new();
    for access in access_parts {
        for read in access.reads() {
            merged.add_read(*read);
        }
        for write in access.writes() {
            merged.add_write(*write);
        }
        for _ in 0..access.exclusive_world_accesses() {
            merged.add_exclusive_world_access();
        }
    }
    if let Err(conflict) = merged.validate_internal() {
        return Err(RuntimeError::Setup {
            message: format!(
                "system '{}' has conflicting param access: {}",
                system_name,
                conflict.diagnostic_message()
            ),
        });
    }
    Ok(merged)
}

macro_rules! build_registered_system {
    (transferable, $func:ident, $($index:tt, $param:ident),*) => {{
        let system_name = std::any::type_name::<Func>().to_string();
        let mut deferred_recorder_class = DeferredRecorderClass::None;
        $(
            deferred_recorder_class = deferred_recorder_class
                .merge(<$param as SystemParamState>::deferred_recorder_class().map_err(
                    |conflict| RuntimeError::Setup {
                        message: format!(
                            "system '{}' has invalid deferred recorder metadata: {}",
                            system_name, conflict
                        ),
                    },
                )?)
                .map_err(|conflict| RuntimeError::Setup {
                    message: format!(
                        "system '{}' has mixed deferred recorder capabilities: {}",
                        system_name, conflict
                    ),
                })?;
        )*
        if deferred_recorder_class == DeferredRecorderClass::LocalDeferred {
            return Err(RuntimeError::Setup {
                message: format!(
                    "system '{}' uses local deferred commands and is not transferable",
                    system_name
                ),
            });
        }
        let states = (
            $(<$param as SystemParamState>::init_state().map_err(|source| RuntimeError::Param {
                system: system_name.clone(),
                source,
            })?,)*
        );
        let query_access_parts = vec![
            $(<$param as SystemParamState>::access(&states.$index),)*
        ];
        validate_borrow_access(&system_name, &query_access_parts)?;
        let access_parts = query_access_parts
            .into_iter()
            .map(query_access_to_system_access)
            .collect::<Vec<_>>();
        let access = merge_access(&system_name, &access_parts)?;
        let param_slots = vec![
            $(<$param as SystemParamState>::slot_descriptor(),)*
        ];
        let system_name_for_serial = system_name.clone();
        let system_name_for_worker = system_name.clone();
        let system_name_for_worker_prepare = system_name.clone();
        let runner_state = ($func, states);
        let mut registered = RegisteredSystem::new_transferable_worker_capable::<Sched, _>(
            system_name,
            access,
            runner_state,
            move |runner_state, world| {
                let ($func, states) = runner_state;
                let mut mutation_journal = MutationJournal::new(&*world);
                let mut commands = (deferred_recorder_class
                    == DeferredRecorderClass::TransferableDeferred)
                    .then(Commands::new_external_owner);
                let invocation_result = {
                    let context = SystemParamContext::new(
                        world,
                        &mut mutation_journal,
                        None,
                        commands.as_mut(),
                    );
                    $(
                        let $param = unsafe {
                            <$param as SystemParamState>::extract(&mut states.$index, context)
                                .map_err(|source| RuntimeError::Param {
                                    system: system_name_for_serial.clone(),
                                    source,
                                })?
                        };
                    )*
                    catch_unwind(AssertUnwindSafe(|| {
                        $func($($param),*)
                            .into_result()
                            .map_err(|source| RuntimeError::System {
                                system: system_name_for_serial.clone(),
                                source,
                            })
                    }))
                };
                match invocation_result {
                    Ok(result) => {
                        let staged_commands = match deferred_recorder_class {
                            DeferredRecorderClass::None => None,
                            DeferredRecorderClass::TransferableDeferred => Some(
                                commands
                                    .expect("command owner must exist for transferable recorder")
                                    .finalize_external_owner(),
                            ),
                            DeferredRecorderClass::LocalDeferred => unreachable!(
                                "local deferred commands were rejected before registration"
                            ),
                        };
                        mutation_journal.commit(world);
                        result.map(|()| staged_commands)
                    }
                    Err(payload) => {
                        mutation_journal.commit(world);
                        resume_unwind(payload)
                    }
                }
            },
            move |runner_state, lease| {
                let (_func, states) = runner_state;
                let mut context = WorkerPrepareContext::new(lease);
                $(
                    <$param as TransferableSystemParam>::prepare_worker(
                        &states.$index,
                        &mut context,
                    ).map_err(|source| RuntimeError::Param {
                        system: system_name_for_worker_prepare.clone(),
                        source,
                    })?;
                )*
                Ok(context.finish())
            },
            move |runner_state, prepared, capacity| {
                let ($func, states) = runner_state;
                let mut mutation_journal =
                    MutationJournal::new_concurrent(prepared.base_cursor(), capacity);
                let mut panic_phase = WorkerPanicPhase::Framework;
                let invocation_result = catch_unwind(AssertUnwindSafe(|| -> Result<Option<TransferableCommandBuffer>> {
                    let mut commands = (deferred_recorder_class
                        == DeferredRecorderClass::TransferableDeferred)
                        .then(Commands::new_external_owner);
                    let context = SystemParamContext::new_worker(
                        prepared.authority(),
                        &mut mutation_journal,
                        commands.as_mut(),
                    );
                    $(
                        let $param = match unsafe {
                            <$param as SystemParamState>::extract(&mut states.$index, context)
                        } {
                            Ok(value) => value,
                            Err(error) => panic!(
                                "worker parameter extraction failed after successful preparation: {error}"
                            ),
                        };
                    )*
                    panic_phase = WorkerPanicPhase::User;
                    let body_result = $func($($param),*)
                        .into_result()
                        .map_err(|source| RuntimeError::System {
                            system: system_name_for_worker.clone(),
                            source,
                        });
                    panic_phase = WorkerPanicPhase::Framework;
                    body_result?;
                    let staged_commands = match deferred_recorder_class {
                        DeferredRecorderClass::None => None,
                        DeferredRecorderClass::TransferableDeferred => Some(
                            commands
                                .expect("command owner must exist for transferable recorder")
                                .finalize_external_owner(),
                        ),
                        DeferredRecorderClass::LocalDeferred => unreachable!(
                            "local deferred commands were rejected before registration"
                        ),
                    };
                    Ok(staged_commands)
                }));
                let outcome = match invocation_result {
                    Ok(Ok(buffer)) => WorkerInvocationOutcome::Success(buffer),
                    Ok(Err(error)) => WorkerInvocationOutcome::Error(error),
                    Err(payload) => WorkerInvocationOutcome::Panic {
                        payload,
                        phase: panic_phase,
                    },
                };
                WorkerInvocationReport {
                    journal: mutation_journal,
                    outcome,
                }
            },
        )?;
        registered.set_deferred_recorder_class(deferred_recorder_class);
        registered.set_param_slots(param_slots);
        Ok(registered)
    }};
    (local, $func:ident, $($index:tt, $param:ident),*) => {{
        let system_name = std::any::type_name::<Func>().to_string();
        let mut deferred_recorder_class = DeferredRecorderClass::None;
        $(
            deferred_recorder_class = deferred_recorder_class
                .merge(<$param as SystemParamState>::deferred_recorder_class().map_err(
                    |conflict| RuntimeError::Setup {
                        message: format!(
                            "system '{}' has invalid deferred recorder metadata: {}",
                            system_name, conflict
                        ),
                    },
                )?)
                .map_err(|conflict| RuntimeError::Setup {
                    message: format!(
                        "system '{}' has mixed deferred recorder capabilities: {}",
                        system_name, conflict
                    ),
                })?;
        )*
        let mut states = (
            $(<$param as SystemParamState>::init_state().map_err(|source| RuntimeError::Param {
                system: system_name.clone(),
                source,
            })?,)*
        );
        let query_access_parts = vec![
            $(<$param as SystemParamState>::access(&states.$index),)*
        ];
        validate_borrow_access(&system_name, &query_access_parts)?;
        let access_parts = query_access_parts
            .into_iter()
            .map(query_access_to_system_access)
            .collect::<Vec<_>>();
        let access = merge_access(&system_name, &access_parts)?;
        let param_slots = vec![
            $(<$param as SystemParamState>::slot_descriptor(),)*
        ];
        let system_name_for_run = system_name.clone();
        let mut registered = RegisteredSystem::new_invoker_thread_only::<Sched>(
            system_name,
            access,
            move |world| {
                let mut mutation_journal = MutationJournal::new(&*world);
                let mut local_commands = (deferred_recorder_class
                    == DeferredRecorderClass::LocalDeferred)
                    .then(LocalCommands::new_external_owner);
                let mut commands = (deferred_recorder_class
                    == DeferredRecorderClass::TransferableDeferred)
                    .then(Commands::new_external_owner);
                let invocation_result = {
                    let context = SystemParamContext::new(
                        world,
                        &mut mutation_journal,
                        local_commands.as_mut(),
                        commands.as_mut(),
                    );
                    $(
                        let $param = unsafe {
                            <$param as SystemParamState>::extract(&mut states.$index, context)
                                .map_err(|source| RuntimeError::Param {
                                    system: system_name_for_run.clone(),
                                    source,
                                })?
                        };
                    )*
                    catch_unwind(AssertUnwindSafe(|| {
                        $func($($param),*)
                            .into_result()
                            .map_err(|source| RuntimeError::System {
                                system: system_name_for_run.clone(),
                                source,
                            })
                    }))
                };
                match invocation_result {
                    Ok(result) => {
                        let staged_commands = match deferred_recorder_class {
                            DeferredRecorderClass::None => None,
                            DeferredRecorderClass::LocalDeferred => Some(DeferredCommandBuffer::Local(
                                local_commands
                                    .expect("local command owner must exist for local recorder")
                                    .finalize_external_owner(),
                            )),
                            DeferredRecorderClass::TransferableDeferred => Some(
                                DeferredCommandBuffer::Transferable(
                                    commands
                                        .expect("command owner must exist for transferable recorder")
                                        .finalize_external_owner(),
                                ),
                            ),
                        };
                        mutation_journal.commit(world);
                        result.map(|()| staged_commands)
                    }
                    Err(payload) => {
                        mutation_journal.commit(world);
                        resume_unwind(payload)
                    }
                }
            },
        )?;
        registered.set_deferred_recorder_class(deferred_recorder_class);
        registered.set_param_slots(param_slots);
        Ok(registered)
    }};
}

macro_rules! impl_into_system {
    ($(($index:tt, $param:ident)),* $(,)?) => {
        #[allow(unused_mut, unused_variables, non_snake_case)]
        impl<Func, R, $($param),*> IntoSystem<fn($($param),*) -> R> for Func
        where
            Func: FnMut($($param),*) -> R
                + for<'world, 'state> FnMut($(<$param as SystemParamState>::Item<'world, 'state>),*) -> R
                + Send
                + 'static,
            $($param: SystemParam + crate::system::TransferableSystemParam,)*
            $(<$param as SystemParam>::State: Send,)*
            R: SystemOutput,
        {
            fn into_registered_system<Sched: ScheduleLabel>(
                self,
            ) -> Result<RegisteredSystem> {
                let func = self;
                build_registered_system!(transferable, func, $($index, $param),*)
            }
        }

        #[allow(unused_mut, unused_variables, non_snake_case)]
        impl<Func, R, $($param),*> IntoSystem<fn($($param),*) -> R>
            for InvokerThreadSystem<Func>
        where
            Func: FnMut($($param),*) -> R
                + for<'world, 'state> FnMut($(<$param as SystemParamState>::Item<'world, 'state>),*) -> R
                + 'static,
            $($param: SystemParam,)*
            R: SystemOutput,
        {
            fn into_registered_system<Sched: ScheduleLabel>(
                self,
            ) -> Result<RegisteredSystem> {
                let mut func = self.0;
                build_registered_system!(local, func, $($index, $param),*)
            }
        }
    };
}

impl_into_system!();
impl_into_system!((0, A));
impl_into_system!((0, A), (1, B));
impl_into_system!((0, A), (1, B), (2, C));
impl_into_system!((0, A), (1, B), (2, C), (3, D));
impl_into_system!((0, A), (1, B), (2, C), (3, D), (4, E));
impl_into_system!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F));
impl_into_system!((0, A), (1, B), (2, C), (3, D), (4, E), (5, F), (6, G));
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J),
    (10, K)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J),
    (10, K),
    (11, L)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J),
    (10, K),
    (11, L),
    (12, M)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J),
    (10, K),
    (11, L),
    (12, M),
    (13, N)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J),
    (10, K),
    (11, L),
    (12, M),
    (13, N),
    (14, O)
);
impl_into_system!(
    (0, A),
    (1, B),
    (2, C),
    (3, D),
    (4, E),
    (5, F),
    (6, G),
    (7, H),
    (8, I),
    (9, J),
    (10, K),
    (11, L),
    (12, M),
    (13, N),
    (14, O),
    (15, P)
);

pub struct Runtime {
    scheduler: ScheduleRegistry,
    deferred_commands: DeferredCommands,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            scheduler: ScheduleRegistry::new(),
            deferred_commands: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn add_systems<L, S, Marker>(&mut self, _schedule: L, systems: S) -> Result<&mut Self>
    where
        L: ScheduleLabel,
        S: IntoSystemConfigs<Marker>,
    {
        let registered = systems.register::<L>()?;
        self.scheduler.add_systems(registered)?;
        Ok(self)
    }

    pub fn validate(&mut self) -> Result<()> {
        self.scheduler.validate().map_err(Into::into)
    }

    pub fn inspect_schedule<L: ScheduleLabel>(&mut self) -> Result<Option<ScheduleInspection>> {
        self.validate()?;
        let plan = self.scheduler.plan_for::<L>()?.cloned();
        Ok(plan.map(|plan| ScheduleInspection::from_plan(&plan, self.scheduler.systems())))
    }

    pub fn run_schedule<L: ScheduleLabel>(&mut self, world: &mut World) -> Result<()> {
        self.run_schedule_with_deferred_publication_frontier::<L, _, _>(
            world,
            |_frontier, _world| Ok::<(), RuntimeError>(()),
        )
    }

    pub fn run_schedule_with_deferred_publication_frontier<L, F, E>(
        &mut self,
        world: &mut World,
        mut on_frontier: F,
    ) -> Result<()>
    where
        L: ScheduleLabel,
        F: FnMut(DeferredPublicationFrontier, &mut World) -> std::result::Result<(), E>,
        E: Into<Box<dyn Error + Send + Sync>> + 'static,
    {
        let _unwind_guard = DeferredCommandsUnwindGuard::new(self.deferred_commands.clone());

        if let Err(err) = self.validate() {
            self.discard_deferred_commands();
            return Err(err);
        }

        let plan = match self.scheduler.plan_for::<L>() {
            Ok(Some(plan)) => plan.clone(),
            Ok(None) => {
                return Ok(());
            }
            Err(err) => {
                self.discard_deferred_commands();
                return Err(err.into());
            }
        };
        let mut next_frontier = 0usize;
        for (reference_rank, system_index) in plan.reference_system_indices.iter().enumerate() {
            let outcome = {
                let Some(system) = self.scheduler.systems_mut().get_mut(*system_index) else {
                    self.discard_deferred_commands();
                    return Err(RuntimeError::Invariant {
                        message: "execution plan referenced missing system",
                    });
                };
                match system.run(world) {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        self.discard_deferred_commands();
                        return Err(err);
                    }
                }
            };
            self.append_invocation_outcome(outcome);

            let cut = reference_rank.saturating_add(1);
            while plan
                .publication_frontiers
                .get(next_frontier)
                .is_some_and(|frontier| frontier.cut == cut)
            {
                if let Err(err) = self.publish_deferred_commands(world) {
                    self.discard_deferred_commands();
                    return Err(err);
                }
                if let Err(err) = on_frontier(
                    DeferredPublicationFrontier {
                        schedule: plan.label,
                        ordinal: next_frontier,
                    },
                    world,
                ) {
                    self.discard_deferred_commands();
                    return Err(RuntimeError::Boundary { source: err.into() });
                }
                next_frontier = next_frontier.saturating_add(1);
            }
        }

        if next_frontier != plan.publication_frontiers.len() {
            self.discard_deferred_commands();
            return Err(RuntimeError::Invariant {
                message: "publication plan contained an unreached frontier",
            });
        }
        Ok(())
    }

    fn publish_deferred_commands(&self, world: &mut World) -> Result<()> {
        world.begin_deferred_publication();
        let pending_commands = std::mem::take(&mut *self.deferred_commands.borrow_mut());
        for commands in pending_commands {
            match commands {
                DeferredCommandBuffer::Local(commands) => commands.apply(world)?,
                DeferredCommandBuffer::Transferable(commands) => commands.apply(world)?,
            }
        }
        Ok(())
    }

    fn append_invocation_outcome(&self, outcome: InvocationOutcome) {
        let buffer = match outcome {
            InvocationOutcome::None => return,
            InvocationOutcome::Local(commands) => DeferredCommandBuffer::Local(commands),
            InvocationOutcome::Transferable(commands) => {
                DeferredCommandBuffer::Transferable(commands)
            }
        };
        self.deferred_commands.borrow_mut().push(buffer);
    }

    fn discard_deferred_commands(&self) {
        self.deferred_commands.borrow_mut().clear();
    }
}

fn query_access_to_system_access(access: QueryAccess) -> SystemAccess {
    let mut system_access = SystemAccess::new();
    for read in access.component_reads() {
        system_access.add_read(AccessKey::component_by_id(read.type_id(), read.name()));
    }
    for read in access.removed_component_reads() {
        system_access.add_read(AccessKey::removed_component_by_id(
            read.type_id(),
            read.name(),
        ));
    }
    for write in access.component_writes() {
        system_access.add_write(AccessKey::component_by_id(write.type_id(), write.name()));
    }
    for read in access.resource_reads() {
        system_access.add_read(AccessKey::resource_by_id(read.type_id(), read.name()));
    }
    for write in access.resource_writes() {
        system_access.add_write(AccessKey::resource_by_id(write.type_id(), write.name()));
    }
    if access.deferred_structural_mutation() {
        system_access.add_write(AccessKey::structural("world_structure"));
    }
    for _ in 0..access.exclusive_world_accesses() {
        system_access.add_exclusive_world_access();
    }
    system_access
}

#[cfg(test)]
mod tests {
    use super::Runtime;
    use crate::{Res, ResMut, Resource, ScheduleLabel, World};

    macro_rules! define_u32_resource {
        ($name:ident) => {
            #[derive(Debug, Copy, Clone, PartialEq, Eq)]
            struct $name(pub u32);
            impl Resource for $name {}
        };
    }

    define_u32_resource!(R0);
    define_u32_resource!(R1);
    define_u32_resource!(R2);
    define_u32_resource!(R3);
    define_u32_resource!(R4);
    define_u32_resource!(R5);
    define_u32_resource!(R6);
    define_u32_resource!(R7);
    define_u32_resource!(R8);
    define_u32_resource!(R9);
    define_u32_resource!(R10);
    define_u32_resource!(R11);
    define_u32_resource!(R12);
    define_u32_resource!(R13);
    define_u32_resource!(R14);
    define_u32_resource!(Sum);
    define_u32_resource!(Counter);

    #[derive(Debug, Copy, Clone)]
    struct MaxAritySchedule;
    impl ScheduleLabel for MaxAritySchedule {}

    #[derive(Debug, Copy, Clone)]
    struct MaxTupleSchedule;
    impl ScheduleLabel for MaxTupleSchedule {}

    #[allow(clippy::too_many_arguments)]
    fn max_arity_system(
        r0: Res<R0>,
        r1: Res<R1>,
        r2: Res<R2>,
        r3: Res<R3>,
        r4: Res<R4>,
        r5: Res<R5>,
        r6: Res<R6>,
        r7: Res<R7>,
        r8: Res<R8>,
        r9: Res<R9>,
        r10: Res<R10>,
        r11: Res<R11>,
        r12: Res<R12>,
        r13: Res<R13>,
        r14: Res<R14>,
        mut sum: ResMut<Sum>,
    ) {
        sum.0 = r0.0
            + r1.0
            + r2.0
            + r3.0
            + r4.0
            + r5.0
            + r6.0
            + r7.0
            + r8.0
            + r9.0
            + r10.0
            + r11.0
            + r12.0
            + r13.0
            + r14.0;
    }

    fn bump_counter(mut counter: ResMut<Counter>) {
        counter.0 = counter.0.saturating_add(1);
    }

    #[test]
    fn supports_max_function_system_arity_sixteen() {
        let mut world = World::new();
        world.insert_resource(R0(1));
        world.insert_resource(R1(2));
        world.insert_resource(R2(3));
        world.insert_resource(R3(4));
        world.insert_resource(R4(5));
        world.insert_resource(R5(6));
        world.insert_resource(R6(7));
        world.insert_resource(R7(8));
        world.insert_resource(R8(9));
        world.insert_resource(R9(10));
        world.insert_resource(R10(11));
        world.insert_resource(R11(12));
        world.insert_resource(R12(13));
        world.insert_resource(R13(14));
        world.insert_resource(R14(15));
        world.insert_resource(Sum(0));

        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(MaxAritySchedule, max_arity_system);
        runtime
            .run_schedule::<MaxAritySchedule>(&mut world)
            .unwrap();
        assert_eq!(world.resource::<Sum>().unwrap().0, 120);
    }

    #[test]
    fn supports_max_tuple_registration_arity_sixteen() {
        let mut world = World::new();
        world.insert_resource(Counter(0));
        let mut runtime = Runtime::new();
        let _ = runtime.add_systems(
            MaxTupleSchedule,
            (
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
                bump_counter,
            ),
        );
        runtime
            .run_schedule::<MaxTupleSchedule>(&mut world)
            .unwrap();
        assert_eq!(world.resource::<Counter>().unwrap().0, 16);
    }
}

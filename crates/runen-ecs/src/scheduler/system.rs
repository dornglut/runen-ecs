use crate::World;
use crate::commands::TransferableCommandBuffer;
use crate::errors::RuntimeError;
use crate::scheduler::access::{AccessConflict, SystemAccess};
use crate::scheduler::label::{ScheduleKey, ScheduleLabel, SystemSet, SystemSetKey};
use crate::system::DeferredRecorderClass;
use crate::system::ExecutionMobility;
use crate::system::OrderingDirection;
use crate::system::runtime::{DeferredCommandBuffer, InvocationOutcome};
use std::num::NonZeroU64;

pub(crate) struct TransferableSystemRunner {
    run: Box<
        dyn FnMut(&mut World) -> Result<Option<TransferableCommandBuffer>, RuntimeError> + Send,
    >,
}

pub(crate) struct InvokerThreadSystemRunner {
    run: Box<dyn FnMut(&mut World) -> Result<Option<DeferredCommandBuffer>, RuntimeError>>,
}

pub(crate) enum RegisteredSystemRunner {
    Transferable(TransferableSystemRunner),
    InvokerThreadOnly(InvokerThreadSystemRunner),
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<TransferableSystemRunner>();
};

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub struct SystemId(NonZeroU64);

impl SystemId {
    pub(crate) const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamSlotDescriptor {
    pub name: Option<&'static str>,
    pub kind: &'static str,
    pub label: &'static str,
    pub type_name: &'static str,
    pub children: Vec<ParamSlotDescriptor>,
}

impl ParamSlotDescriptor {
    pub fn leaf(kind: &'static str, label: &'static str, type_name: &'static str) -> Self {
        Self {
            name: None,
            kind,
            label,
            type_name,
            children: Vec::new(),
        }
    }

    pub fn group(
        kind: &'static str,
        label: &'static str,
        type_name: &'static str,
        children: Vec<ParamSlotDescriptor>,
    ) -> Self {
        Self {
            name: None,
            kind,
            label,
            type_name,
            children,
        }
    }

    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    pub fn named_child(name: &'static str, descriptor: ParamSlotDescriptor) -> Self {
        descriptor.with_name(name)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OrderingPresence {
    Optional,
    Required,
}

impl OrderingPresence {
    pub(crate) const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }

    const fn merge(self, other: Self) -> Self {
        if self.is_required() || other.is_required() {
            Self::Required
        } else {
            Self::Optional
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct OrderingDeclaration {
    direction: OrderingDirection,
    target: SystemSetKey,
    presence: OrderingPresence,
}

impl OrderingDeclaration {
    pub(crate) const fn required(direction: OrderingDirection, target: SystemSetKey) -> Self {
        Self {
            direction,
            target,
            presence: OrderingPresence::Required,
        }
    }

    pub(crate) const fn optional(direction: OrderingDirection, target: SystemSetKey) -> Self {
        Self {
            direction,
            target,
            presence: OrderingPresence::Optional,
        }
    }

    pub(crate) const fn direction(self) -> OrderingDirection {
        self.direction
    }

    pub(crate) const fn target(self) -> SystemSetKey {
        self.target
    }

    pub(crate) const fn presence(self) -> OrderingPresence {
        self.presence
    }

    pub(crate) fn normalize_into(declarations: &mut Vec<Self>, declaration: Self) {
        if let Some(existing) = declarations.iter_mut().find(|existing| {
            existing.direction == declaration.direction && existing.target == declaration.target
        }) {
            existing.presence = existing.presence.merge(declaration.presence);
            return;
        }
        declarations.push(declaration);
    }
}

pub struct RegisteredSystem {
    id: SystemId,
    name: String,
    label: ScheduleKey,
    sets: Vec<SystemSetKey>,
    ordering_declarations: Vec<OrderingDeclaration>,
    param_slots: Vec<ParamSlotDescriptor>,
    access: SystemAccess,
    deferred_recorder_class: DeferredRecorderClass,
    run: RegisteredSystemRunner,
}

impl RegisteredSystem {
    pub fn new<L>(
        name: impl Into<String>,
        access: SystemAccess,
        mut run: impl FnMut(&mut World) -> Result<(), RuntimeError> + 'static,
    ) -> Result<Self, RuntimeError>
    where
        L: ScheduleLabel,
    {
        let name = name.into();
        access
            .validate_internal()
            .map_err(|conflict| internal_access_error(&name, &conflict))?;
        Self::new_invoker_thread_only::<L>(name, access, move |world| run(world).map(|()| None))
    }

    pub(crate) fn new_transferable<L>(
        name: impl Into<String>,
        access: SystemAccess,
        run: impl FnMut(&mut World) -> Result<Option<TransferableCommandBuffer>, RuntimeError>
        + Send
        + 'static,
    ) -> Result<Self, RuntimeError>
    where
        L: ScheduleLabel,
    {
        let name = name.into();
        access
            .validate_internal()
            .map_err(|conflict| internal_access_error(&name, &conflict))?;
        Ok(Self {
            id: SystemId::new(NonZeroU64::new(1).expect("literal system id is non-zero")),
            name,
            label: L::key(),
            sets: Vec::new(),
            ordering_declarations: Vec::new(),
            param_slots: Vec::new(),
            access,
            deferred_recorder_class: DeferredRecorderClass::None,
            run: RegisteredSystemRunner::Transferable(TransferableSystemRunner {
                run: Box::new(run),
            }),
        })
    }

    pub(crate) fn new_invoker_thread_only<L>(
        name: impl Into<String>,
        access: SystemAccess,
        run: impl FnMut(&mut World) -> Result<Option<DeferredCommandBuffer>, RuntimeError> + 'static,
    ) -> Result<Self, RuntimeError>
    where
        L: ScheduleLabel,
    {
        let name = name.into();
        access
            .validate_internal()
            .map_err(|conflict| internal_access_error(&name, &conflict))?;
        Ok(Self {
            id: SystemId::new(NonZeroU64::new(1).expect("literal system id is non-zero")),
            name,
            label: L::key(),
            sets: Vec::new(),
            ordering_declarations: Vec::new(),
            param_slots: Vec::new(),
            access,
            deferred_recorder_class: DeferredRecorderClass::None,
            run: RegisteredSystemRunner::InvokerThreadOnly(InvokerThreadSystemRunner {
                run: Box::new(run),
            }),
        })
    }

    pub fn with_set<S: SystemSet>(mut self) -> Self {
        self.with_set_key(S::key());
        self
    }

    pub fn with_set_key(&mut self, key: SystemSetKey) -> &mut Self {
        if !self.sets.contains(&key) {
            self.sets.push(key);
        }
        self
    }

    pub fn before_set<S: SystemSet>(mut self) -> Self {
        self.before_set_key(S::key());
        self
    }

    pub fn before_set_key(&mut self, key: SystemSetKey) -> &mut Self {
        self.add_ordering_declaration(OrderingDeclaration::required(
            OrderingDirection::Before,
            key,
        ));
        self
    }

    pub fn after_set<S: SystemSet>(mut self) -> Self {
        self.after_set_key(S::key());
        self
    }

    pub fn after_set_key(&mut self, key: SystemSetKey) -> &mut Self {
        self.add_ordering_declaration(OrderingDeclaration::required(OrderingDirection::After, key));
        self
    }

    pub(crate) fn add_ordering_declaration(&mut self, declaration: OrderingDeclaration) {
        OrderingDeclaration::normalize_into(&mut self.ordering_declarations, declaration);
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn id(&self) -> SystemId {
        self.id
    }

    pub fn label(&self) -> ScheduleKey {
        self.label
    }

    pub fn sets(&self) -> &[SystemSetKey] {
        &self.sets
    }

    pub(crate) fn ordering_declarations(&self) -> &[OrderingDeclaration] {
        &self.ordering_declarations
    }

    pub fn access(&self) -> &SystemAccess {
        &self.access
    }

    pub(crate) fn deferred_recorder_class(&self) -> DeferredRecorderClass {
        self.deferred_recorder_class
    }

    pub(crate) fn set_deferred_recorder_class(&mut self, class: DeferredRecorderClass) {
        self.deferred_recorder_class = class;
    }

    pub fn set_param_slots(&mut self, param_slots: Vec<ParamSlotDescriptor>) {
        self.param_slots = param_slots;
    }

    pub fn param_slots(&self) -> &[ParamSlotDescriptor] {
        &self.param_slots
    }

    pub(crate) fn execution_mobility(&self) -> ExecutionMobility {
        match &self.run {
            RegisteredSystemRunner::Transferable(_) => ExecutionMobility::Transferable,
            RegisteredSystemRunner::InvokerThreadOnly(_) => ExecutionMobility::InvokerThreadOnly,
        }
    }

    pub(crate) fn run(&mut self, world: &mut World) -> Result<InvocationOutcome, RuntimeError> {
        match &mut self.run {
            RegisteredSystemRunner::Transferable(runner) => {
                runner.run.as_mut()(world).map(|buffer| {
                    buffer
                        .map(InvocationOutcome::Transferable)
                        .unwrap_or(InvocationOutcome::None)
                })
            }
            RegisteredSystemRunner::InvokerThreadOnly(runner) => {
                runner.run.as_mut()(world).map(|buffer| match buffer {
                    Some(DeferredCommandBuffer::Local(commands)) => {
                        InvocationOutcome::Local(commands)
                    }
                    Some(DeferredCommandBuffer::Transferable(commands)) => {
                        InvocationOutcome::Transferable(commands)
                    }
                    None => InvocationOutcome::None,
                })
            }
        }
    }

    pub(crate) fn assign_id(&mut self, id: SystemId) {
        self.id = id;
    }
}

fn internal_access_error(system_name: &str, conflict: &AccessConflict) -> RuntimeError {
    RuntimeError::Setup {
        message: format!(
            "system '{system_name}' has conflicting access: {}",
            conflict.diagnostic_message()
        ),
    }
}

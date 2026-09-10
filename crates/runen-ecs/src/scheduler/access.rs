use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AccessDomain {
    Component,
    RemovedComponent,
    Resource,
    Structural,
    World,
}

#[derive(Debug, Copy, Clone)]
pub struct AccessKey {
    domain: AccessDomain,
    type_id: Option<TypeId>,
    name: &'static str,
}

impl AccessKey {
    pub fn component<T: 'static>(name: &'static str) -> Self {
        Self::component_by_id(TypeId::of::<T>(), name)
    }

    pub fn component_by_id(type_id: TypeId, name: &'static str) -> Self {
        Self {
            domain: AccessDomain::Component,
            type_id: Some(type_id),
            name,
        }
    }

    pub fn removed_component<T: 'static>(name: &'static str) -> Self {
        Self::removed_component_by_id(TypeId::of::<T>(), name)
    }

    pub fn removed_component_by_id(type_id: TypeId, name: &'static str) -> Self {
        Self {
            domain: AccessDomain::RemovedComponent,
            type_id: Some(type_id),
            name,
        }
    }

    pub fn resource<T: 'static>(name: &'static str) -> Self {
        Self::resource_by_id(TypeId::of::<T>(), name)
    }

    pub fn resource_by_id(type_id: TypeId, name: &'static str) -> Self {
        Self {
            domain: AccessDomain::Resource,
            type_id: Some(type_id),
            name,
        }
    }

    pub fn structural(name: &'static str) -> Self {
        Self {
            domain: AccessDomain::Structural,
            type_id: None,
            name,
        }
    }

    pub fn world(name: &'static str) -> Self {
        Self {
            domain: AccessDomain::World,
            type_id: None,
            name,
        }
    }

    pub fn domain(&self) -> AccessDomain {
        self.domain
    }

    pub fn type_id(&self) -> Option<TypeId> {
        self.type_id
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn diagnostic_label(&self) -> String {
        let domain = match self.domain {
            AccessDomain::Component => "component",
            AccessDomain::RemovedComponent => "removed component",
            AccessDomain::Resource => "resource",
            AccessDomain::Structural => "structural access",
            AccessDomain::World => "world access",
        };
        format!("{domain} '{}'", self.name)
    }
}

impl PartialEq for AccessKey {
    fn eq(&self, other: &Self) -> bool {
        if self.domain != other.domain {
            return false;
        }
        match self.domain {
            AccessDomain::Structural | AccessDomain::World => self.name == other.name,
            _ => self.type_id == other.type_id,
        }
    }
}

impl Eq for AccessKey {}

impl Hash for AccessKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.domain.hash(state);
        match self.domain {
            AccessDomain::Structural | AccessDomain::World => self.name.hash(state),
            _ => self.type_id.hash(state),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConflictKind {
    ReadWrite,
    WriteWrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessConflict {
    pub key: AccessKey,
    pub kind: ConflictKind,
}

impl ConflictKind {
    pub fn diagnostic_label(self) -> &'static str {
        match self {
            ConflictKind::ReadWrite => "read/write",
            ConflictKind::WriteWrite => "write/write",
        }
    }
}

impl AccessConflict {
    pub fn diagnostic_message(&self) -> String {
        format!(
            "{} conflict on {}",
            self.kind.diagnostic_label(),
            self.key.diagnostic_label()
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct SystemAccess {
    reads: HashSet<AccessKey>,
    writes: HashSet<AccessKey>,
    read_order: Vec<AccessKey>,
    write_order: Vec<AccessKey>,
    exclusive_world_accesses: usize,
    has_immediate_world_access: bool,
}

impl SystemAccess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reads(&self) -> &HashSet<AccessKey> {
        &self.reads
    }

    pub fn writes(&self) -> &HashSet<AccessKey> {
        &self.writes
    }

    pub fn exclusive_world_accesses(&self) -> usize {
        self.exclusive_world_accesses
    }

    pub fn add_exclusive_world_access(&mut self) {
        self.exclusive_world_accesses = self.exclusive_world_accesses.saturating_add(1);
    }

    pub fn add_read(&mut self, key: AccessKey) {
        if key.domain() != AccessDomain::Structural {
            self.has_immediate_world_access = true;
        }
        if self.reads.insert(key) {
            self.read_order.push(key);
        }
    }

    pub fn add_write(&mut self, key: AccessKey) {
        if key.domain() != AccessDomain::Structural {
            self.has_immediate_world_access = true;
        }
        if self.writes.insert(key) {
            self.write_order.push(key);
        }
    }

    pub fn with_read(mut self, key: AccessKey) -> Self {
        self.add_read(key);
        self
    }

    pub fn with_write(mut self, key: AccessKey) -> Self {
        self.add_write(key);
        self
    }

    pub fn conflicts_with(&self, other: &Self) -> Vec<AccessConflict> {
        let mut conflicts = Vec::new();
        for key in self.ordered_conflicts(&self.write_order, &other.writes) {
            if key.domain() == AccessDomain::Structural {
                continue;
            }
            conflicts.push(AccessConflict {
                key: *key,
                kind: ConflictKind::WriteWrite,
            });
        }
        for key in self.ordered_conflicts(&self.write_order, &other.reads) {
            conflicts.push(AccessConflict {
                key: *key,
                kind: ConflictKind::ReadWrite,
            });
        }
        for key in self.ordered_conflicts(&self.read_order, &other.writes) {
            conflicts.push(AccessConflict {
                key: *key,
                kind: ConflictKind::ReadWrite,
            });
        }
        if (self.exclusive_world_accesses > 0
            && (other.exclusive_world_accesses > 0 || other.has_immediate_world_access))
            || (other.exclusive_world_accesses > 0 && self.has_immediate_world_access)
        {
            conflicts.push(AccessConflict {
                key: AccessKey::world("world"),
                kind: ConflictKind::WriteWrite,
            });
        }
        self.sort_conflicts(&mut conflicts);
        conflicts
    }

    pub fn validate_internal(&self) -> Result<(), AccessConflict> {
        if self.exclusive_world_accesses > 1
            || (self.exclusive_world_accesses == 1 && self.has_immediate_world_access)
        {
            return Err(AccessConflict {
                key: AccessKey::world("world"),
                kind: ConflictKind::WriteWrite,
            });
        }
        let mut conflicts = Vec::new();
        for key in self.ordered_conflicts(&self.read_order, &self.writes) {
            conflicts.push(AccessConflict {
                key: *key,
                kind: ConflictKind::ReadWrite,
            });
        }
        self.sort_conflicts(&mut conflicts);
        conflicts.into_iter().next().map_or(Ok(()), Err)
    }

    fn ordered_conflicts<'a>(
        &self,
        ordered_keys: &'a [AccessKey],
        other_keys: &HashSet<AccessKey>,
    ) -> impl Iterator<Item = &'a AccessKey> {
        ordered_keys.iter().filter(|key| other_keys.contains(key))
    }

    fn sort_conflicts(&self, conflicts: &mut [AccessConflict]) {
        let order = self.access_order_index();
        conflicts.sort_by_key(|conflict| {
            (
                conflict.key.domain(),
                conflict.kind,
                order.get(&conflict.key).copied().unwrap_or(usize::MAX),
                conflict.key.name(),
            )
        });
    }

    fn access_order_index(&self) -> HashMap<AccessKey, usize> {
        let mut order = HashMap::new();
        for key in self.read_order.iter().chain(self.write_order.iter()) {
            let next = order.len();
            order.entry(*key).or_insert(next);
        }
        order
    }
}

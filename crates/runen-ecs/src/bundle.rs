use crate::component::Component;
use crate::entity::Entity;
use crate::storage::ArchetypeRegistry;
use crate::world::World;
use std::any::{Any, TypeId};

#[doc(hidden)]
#[derive(Copy, Clone)]
pub struct BundleComponentDescriptor {
    type_id: TypeId,
    component_name: &'static str,
    register_storage: fn(&mut ArchetypeRegistry),
    insert_value: fn(&mut World, Entity, Box<dyn Any>),
    remove_value: fn(&mut World, Entity) -> Box<dyn Any>,
}

impl BundleComponentDescriptor {
    #[doc(hidden)]
    pub fn of<T: Component>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            component_name: T::component_name(),
            register_storage: register_storage::<T>,
            insert_value: insert_value::<T>,
            remove_value: remove_value::<T>,
        }
    }

    pub(crate) const fn component_type_id(self) -> TypeId {
        self.type_id
    }

    pub(crate) const fn component_name(self) -> &'static str {
        self.component_name
    }

    pub(crate) fn register_storage(self, registry: &mut ArchetypeRegistry) {
        (self.register_storage)(registry);
    }

    pub(crate) fn insert_value(self, world: &mut World, entity: Entity, value: Box<dyn Any>) {
        (self.insert_value)(world, entity, value);
    }

    pub(crate) fn remove_value(self, world: &mut World, entity: Entity) -> Box<dyn Any> {
        (self.remove_value)(world, entity)
    }
}

fn register_storage<T: Component>(registry: &mut ArchetypeRegistry) {
    registry.register_component_type::<T>();
}

fn insert_value<T: Component>(world: &mut World, entity: Entity, value: Box<dyn Any>) {
    let value = value
        .downcast::<Box<T>>()
        .expect("prepared bundle value must match its descriptor");
    world.__commit_insert_component(entity, **value);
}

fn remove_value<T: Component>(world: &mut World, entity: Entity) -> Box<dyn Any> {
    let value = world.__commit_remove_component::<T>(entity);
    Box::new(Box::new(value))
}

pub(crate) struct BundleComponentValue {
    descriptor: BundleComponentDescriptor,
    value: Box<dyn Any>,
}

impl BundleComponentValue {
    fn new<T: Component>(value: T) -> Self {
        Self {
            descriptor: BundleComponentDescriptor::of::<T>(),
            value: Box::new(Box::new(value)),
        }
    }

    pub(crate) const fn descriptor(&self) -> BundleComponentDescriptor {
        self.descriptor
    }

    pub(crate) fn commit_insert(self, world: &mut World, entity: Entity) {
        self.descriptor.insert_value(world, entity, self.value);
    }

    pub(crate) fn from_removed(
        descriptor: BundleComponentDescriptor,
        world: &mut World,
        entity: Entity,
    ) -> Self {
        Self {
            descriptor,
            value: descriptor.remove_value(world, entity),
        }
    }

    fn into_typed<T: Component>(self) -> Option<T> {
        if self.descriptor.component_type_id() != TypeId::of::<T>() {
            return None;
        }
        let value = self.value.downcast::<Box<T>>().ok()?;
        Some(**value)
    }
}

/// Framework-owned value collector used by the low-level [`Bundle`] contract.
///
/// Ordinary users should construct bundles from components, supported tuples, or
/// `#[derive(Bundle)]` rather than using this type directly.
#[doc(hidden)]
#[derive(Default)]
pub struct BundleComponents {
    components: Vec<BundleComponentValue>,
}

impl BundleComponents {
    #[doc(hidden)]
    pub fn new() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    pub fn push<T: Component>(&mut self, value: T) {
        self.components.push(BundleComponentValue::new(value));
    }

    #[doc(hidden)]
    pub fn take<T: Component>(&mut self) -> Option<T> {
        let type_id = TypeId::of::<T>();
        let index = self
            .components
            .iter()
            .position(|component| component.descriptor().component_type_id() == type_id)?;
        self.components.remove(index).into_typed::<T>()
    }

    pub(crate) fn from_values(components: Vec<BundleComponentValue>) -> Self {
        Self { components }
    }

    pub(crate) fn into_values(self) -> Vec<BundleComponentValue> {
        self.components
    }
}

/// A statically described group of components that can be structurally applied
/// by [`crate::World`].
///
/// Safe bundle authoring is provided by components, supported tuples, and
/// `#[derive(Bundle)]`. Manual implementations are a low-level framework
/// extension boundary and are unsupported.
///
/// # Safety
///
/// An implementation must report a deterministic descriptor sequence and must
/// transfer exactly one matching component value for every descriptor, in the
/// same order. `__from_components` must reconstruct exactly that same bundle
/// shape from values previously produced for the reported descriptors. Breaking
/// this contract may violate ECS storage invariants.
pub unsafe trait Bundle: Sized + 'static {
    #[doc(hidden)]
    fn __component_descriptors() -> Vec<BundleComponentDescriptor>;

    #[doc(hidden)]
    fn __into_components(self, components: &mut BundleComponents);

    /// # Safety
    ///
    /// `components` must contain the values for this bundle's declared
    /// descriptor sequence, as guaranteed by the framework after a successful
    /// removal commit.
    #[doc(hidden)]
    unsafe fn __from_components(components: &mut BundleComponents) -> Self;
}

pub(crate) struct PreparedBundle {
    descriptors: Vec<BundleComponentDescriptor>,
    components: Vec<BundleComponentValue>,
}

impl PreparedBundle {
    pub(crate) fn descriptors(&self) -> &[BundleComponentDescriptor] {
        &self.descriptors
    }

    pub(crate) fn into_components(self) -> Vec<BundleComponentValue> {
        self.components
    }
}

pub(crate) fn prepare_bundle<B: Bundle>(bundle: B) -> PreparedBundle {
    let descriptors = B::__component_descriptors();
    let mut components = BundleComponents::new();
    bundle.__into_components(&mut components);
    let values = components.into_values();

    assert_eq!(
        descriptors.len(),
        values.len(),
        "Bundle implementation must transfer exactly one value per descriptor"
    );
    for (descriptor, value) in descriptors.iter().zip(&values) {
        assert_eq!(
            descriptor.component_type_id(),
            value.descriptor().component_type_id(),
            "Bundle descriptor/value type order must match"
        );
    }

    PreparedBundle {
        descriptors,
        components: values,
    }
}

pub(crate) fn bundle_descriptors<B: Bundle>() -> Vec<BundleComponentDescriptor> {
    B::__component_descriptors()
}

unsafe impl<T: Component> Bundle for T {
    fn __component_descriptors() -> Vec<BundleComponentDescriptor> {
        vec![BundleComponentDescriptor::of::<T>()]
    }

    fn __into_components(self, components: &mut BundleComponents) {
        components.push(self);
    }

    unsafe fn __from_components(components: &mut BundleComponents) -> Self {
        components
            .take::<T>()
            .expect("removed bundle component must match its descriptor")
    }
}

macro_rules! impl_bundle_tuple {
    ($(($ty:ident, $var:ident)),+ $(,)?) => {
        unsafe impl<$($ty: Component),+> Bundle for ($($ty,)+) {
            fn __component_descriptors() -> Vec<BundleComponentDescriptor> {
                vec![$(BundleComponentDescriptor::of::<$ty>()),+]
            }

            fn __into_components(self, components: &mut BundleComponents) {
                let ($($var,)+) = self;
                $(components.push($var);)+
            }

            unsafe fn __from_components(components: &mut BundleComponents) -> Self {
                (
                    $(components
                        .take::<$ty>()
                        .expect("removed bundle component must match its descriptor"),)+
                )
            }
        }
    };
}

impl_bundle_tuple!((A, a), (B, b));
impl_bundle_tuple!((A, a), (B, b), (C, c));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d), (E, e));
impl_bundle_tuple!((A, a), (B, b), (C, c), (D, d), (E, e), (F, f));

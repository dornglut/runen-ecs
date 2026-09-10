/// Marker trait for component types.
pub trait Component: 'static {
    fn component_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Marker trait for world resource types.
pub trait Resource: 'static {
    fn resource_name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

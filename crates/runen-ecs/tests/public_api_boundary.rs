const LIB_RS: &str = include_str!("../src/lib.rs");
const PRELUDE_RS: &str = include_str!("../src/prelude.rs");
const QUERY_MOD_RS: &str = include_str!("../src/query/mod.rs");
const QUERY_TRAITS_RS: &str = include_str!("../src/query/traits_and_state.rs");
const QUERY_ACCESS_RS: &str = include_str!("../src/query/access_and_filters.rs");
const SYSTEM_EXTRACT_RS: &str = include_str!("../src/system/extract.rs");
const SYSTEM_PARAMS_RS: &str = include_str!("../src/system/params.rs");
const WORLD_MOD_RS: &str = include_str!("../src/world/mod.rs");
const WORLD_STATE_RS: &str = include_str!("../src/world/state.rs");
const WORLD_CAPABILITY_RS: &str = include_str!("../src/world/capability.rs");
const WORLD_RUNTIME_RS: &str = include_str!("../src/world/runtime.rs");
const CHANGE_TRACKING_RS: &str = include_str!("../src/world/change_tracking.rs");
const BUNDLE_RS: &str = include_str!("../src/bundle.rs");
const COMPONENT_ACCESS_RS: &str = include_str!("../src/world/component/access.rs");
const COMPONENT_REGISTRATION_RS: &str = include_str!("../src/world/component/registration.rs");

#[test]
fn prelude_remains_gameplay_focused() {
    assert!(PRELUDE_RS.contains("Query"));
    assert!(PRELUDE_RS.contains("Res"));
    assert!(!PRELUDE_RS.contains("ResView"));
    assert!(PRELUDE_RS.contains("ResMut"));
    assert!(PRELUDE_RS.contains("Commands"));
    assert!(PRELUDE_RS.contains("Runtime"));

    assert!(!PRELUDE_RS.contains("QueryAccess"));
    assert!(!PRELUDE_RS.contains("QueryTypeAccess"));
    assert!(!PRELUDE_RS.contains("QueryState"));
    assert!(!PRELUDE_RS.contains("QuerySpec"));
    assert!(!PRELUDE_RS.contains("SystemParam"));
    assert!(!PRELUDE_RS.contains("SystemParamError"));
}

#[test]
fn c6_messaging_authority_is_absent_from_ecs_surfaces() {
    const REMOVED: &[&str] = &[
        "BroadcastReader",
        "BroadcastWriter",
        "BroadcastStream",
        "WorkQueueReader",
        "WorkQueueWriter",
        "WorkQueueDrainer",
        "WorkQueueConfig",
        "TickBufferReader",
        "TickBufferWriter",
        "TickBufferDrainer",
        "TickBufferConfig",
        "TickBufferProvenance",
        "MessagingCapability",
        "MessagingFinalizationCounters",
        "current_buffer_tick",
        "finalized_buffer_tick",
        "set_current_buffer_tick",
        "finalize_tick_boundary",
        "finalize_frame_boundary",
    ];

    for source in [
        LIB_RS,
        PRELUDE_RS,
        SYSTEM_EXTRACT_RS,
        SYSTEM_PARAMS_RS,
        WORLD_MOD_RS,
        WORLD_STATE_RS,
        WORLD_CAPABILITY_RS,
        QUERY_ACCESS_RS,
    ] {
        for removed in REMOVED {
            assert!(
                !source.contains(removed),
                "deleted C6 messaging authority leaked through source surface: {removed}"
            );
        }
    }
}

#[test]
fn c7_ownership_lifecycle_and_structural_extraction_are_absent_from_ecs_surfaces() {
    const REMOVED: &[&str] = &[
        "OwnerId",
        "OwnerRole",
        "OwnerState",
        "OwnershipTarget",
        "OwnershipTransferRecord",
        "ResourceOwnerKey",
        "ResourceOwnershipDescriptor",
        "ChangeExtractionFilter",
        "ChangeExtractionWindow",
        "ComponentStructuralDelta",
        "ResourceStructuralDelta",
        "StructuralDeltaBatch",
        "current_frame_index",
        "advance_change_frame",
    ];

    for source in [
        LIB_RS,
        PRELUDE_RS,
        WORLD_MOD_RS,
        WORLD_STATE_RS,
        WORLD_CAPABILITY_RS,
        WORLD_RUNTIME_RS,
    ] {
        for removed in REMOVED {
            assert!(
                !source.contains(removed),
                "deleted C7 authority leaked through ECS source surface: {removed}"
            );
        }
    }

    assert!(!CHANGE_TRACKING_RS.contains("pub frame:"));
}

#[test]
fn low_level_query_extension_is_sealed() {
    assert!(!QUERY_MOD_RS.contains("pub use traits_and_state::QueryData"));
    assert!(!QUERY_MOD_RS.contains("QueryData"));
    assert!(QUERY_MOD_RS.contains("pub use traits_and_state::QuerySpec"));
    assert!(QUERY_TRAITS_RS.contains("pub trait QuerySpec: sealed::QuerySpecSealed"));
    assert!(QUERY_TRAITS_RS.contains("mod sealed"));
}

#[test]
fn low_level_system_param_extension_requires_explicit_unsafe_implementation() {
    assert!(SYSTEM_EXTRACT_RS.contains("pub unsafe trait SystemParam"));
}

#[test]
fn bundle_extension_boundary_is_unsafe_and_does_not_delegate_world_mutation() {
    assert!(BUNDLE_RS.contains("pub unsafe trait Bundle"));
    assert!(!BUNDLE_RS.contains("fn register(world: &mut World)"));
    assert!(!BUNDLE_RS.contains("fn insert(self, world: &mut World"));
    assert!(!BUNDLE_RS.contains("fn remove(world: &mut World"));
}

#[test]
fn obsolete_component_mutation_reach_through_is_removed() {
    assert!(!COMPONENT_ACCESS_RS.contains("fn __insert_component"));
    assert!(!COMPONENT_ACCESS_RS.contains("fn __remove_component"));
    assert!(!COMPONENT_REGISTRATION_RS.contains("pub fn __register_component"));
}

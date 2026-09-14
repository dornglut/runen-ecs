const LIB_RS: &str = include_str!("../src/lib.rs");
const PRELUDE_RS: &str = include_str!("../src/prelude.rs");
const QUERY_MOD_RS: &str = include_str!("../src/query/mod.rs");
const QUERY_TRAITS_RS: &str = include_str!("../src/query/traits_and_state.rs");
const QUERY_ACCESS_RS: &str = include_str!("../src/query/access_and_filters.rs");
const SYSTEM_MOD_RS: &str = include_str!("../src/system/mod.rs");
const SYSTEM_EXTRACT_RS: &str = include_str!("../src/system/extract.rs");
const SYSTEM_PARAMS_RS: &str = include_str!("../src/system/params.rs");
const SYSTEM_RUNTIME_RS: &str = include_str!("../src/system/runtime.rs");
const SCHEDULER_SYSTEM_RS: &str = include_str!("../src/scheduler/system.rs");
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
fn implementation_only_runtime_identity_and_scheduler_types_are_not_publicly_reexported() {
    const INTERNAL_ONLY: &[&str] = &[
        "EntityAllocator",
        "AccessConflict",
        "AccessDomain",
        "AccessKey",
        "ConflictKind",
        "SystemAccess",
        "SystemId",
        "TypeId",
    ];

    for internal in INTERNAL_ONLY {
        assert!(
            !has_identifier(LIB_RS, internal),
            "implementation-only type leaked through the crate root: {internal}"
        );
        assert!(
            !has_identifier(SYSTEM_MOD_RS, internal),
            "implementation-only type leaked through the public system module: {internal}"
        );
    }
}

#[test]
fn normalized_schedule_inspection_is_public_but_private_scheduler_vocabulary_stays_hidden() {
    for public in [
        "ScheduleInspection",
        "ScheduleOrderingResolution",
        "SchedulePrecedenceEdge",
        "SchedulePrecedencePath",
        "SchedulePublicationFrontier",
        "ScheduleAccessAmbiguity",
        "SchedulePairwiseConcurrencyAssessment",
        "ScheduleOrderingCycle",
        "OrderingPresence",
    ] {
        assert!(
            has_identifier(LIB_RS, public),
            "missing root export: {public}"
        );
        assert!(
            has_identifier(SYSTEM_MOD_RS, public),
            "missing system export: {public}"
        );
        assert!(
            !has_identifier(PRELUDE_RS, public),
            "inspection vocabulary leaked into gameplay prelude: {public}"
        );
    }

    for private in [
        "reference_rank",
        "precedence_depth",
        "source_ordinal",
        "frontier_cut",
        "ExecutionStage",
        "worker_id",
        "cohort_id",
    ] {
        assert!(
            !has_identifier(LIB_RS, private),
            "private vocabulary leaked: {private}"
        );
        assert!(
            !has_identifier(SYSTEM_MOD_RS, private),
            "private vocabulary leaked: {private}"
        );
        assert!(
            !has_identifier(PRELUDE_RS, private),
            "private vocabulary leaked: {private}"
        );
    }
}

fn has_identifier(source: &str, identifier: &str) -> bool {
    source
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == identifier)
}

#[test]
fn downstream_macro_support_contracts_remain_root_reachable() {
    const REQUIRED: &[&str] = &[
        "BundleComponentDescriptor",
        "BundleComponents",
        "ParamSlotDescriptor",
        "QueryAccess",
        "SystemParam",
        "SystemParamContext",
        "SystemParamError",
    ];

    for required in REQUIRED {
        assert!(
            LIB_RS.contains(required),
            "downstream macro support contract disappeared from the crate root: {required}"
        );
    }
}

#[test]
fn deferred_recorder_metadata_is_hidden_but_macro_reachable() {
    assert!(LIB_RS.contains("DeferredRecorderClass"));
    assert!(SYSTEM_EXTRACT_RS.contains("#[doc(hidden)]\n#[derive(Debug, Copy, Clone, PartialEq, Eq)]\npub enum DeferredRecorderClass"));
    assert!(!PRELUDE_RS.contains("DeferredRecorderClass"));
}

#[test]
fn deferred_publication_api_has_no_physical_stage_compatibility_surface() {
    for obsolete in [
        "DeferredApplyBoundary",
        "run_schedule_with_deferred_apply_boundary",
        "ExecutionStage",
        "flush_stage_commands",
        "begin_stage_command_flush",
    ] {
        assert!(
            !LIB_RS.contains(obsolete),
            "obsolete API remains: {obsolete}"
        );
        assert!(
            !SYSTEM_MOD_RS.contains(obsolete),
            "obsolete system API remains: {obsolete}"
        );
        assert!(
            !SYSTEM_RUNTIME_RS.contains(obsolete),
            "obsolete runtime API remains: {obsolete}"
        );
    }
    assert!(LIB_RS.contains("DeferredPublicationFrontier"));
    assert!(SYSTEM_RUNTIME_RS.contains("run_schedule_with_deferred_publication_frontier"));
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
    assert!(QUERY_MOD_RS.contains("pub use traits_and_state::QuerySpec"));
    assert!(QUERY_TRAITS_RS.contains("pub trait QuerySpec: sealed::QuerySpecSealed"));
    assert!(QUERY_TRAITS_RS.contains("mod sealed"));
    assert!(QUERY_TRAITS_RS.contains("pub(crate) unsafe trait TransferableQueryData"));
    assert!(!QUERY_MOD_RS.contains("pub use traits_and_state::TransferableQueryData"));
}

#[test]
fn low_level_system_param_extension_requires_explicit_unsafe_implementation() {
    assert!(SYSTEM_EXTRACT_RS.contains("pub unsafe trait SystemParam"));
}

#[test]
fn transferable_system_param_is_hidden_but_macro_reachable() {
    assert!(LIB_RS.contains("TransferableSystemParam"));
    assert!(SYSTEM_MOD_RS.contains("TransferableSystemParam"));
    assert!(SYSTEM_EXTRACT_RS.contains("#[doc(hidden)]\npub unsafe trait TransferableSystemParam"));
    assert!(!PRELUDE_RS.contains("TransferableSystemParam"));
    assert!(!LIB_RS.contains("TransferableQueryData"));
    assert!(!LIB_RS.contains("TransferableQueryFilter"));
    assert!(!PRELUDE_RS.contains("TransferableQueryData"));
    assert!(!PRELUDE_RS.contains("TransferableQueryFilter"));
    assert!(QUERY_MOD_RS.contains("pub(crate) use"));
}

#[test]
fn execution_mobility_exposes_only_authoring_and_diagnostic_vocabulary() {
    for public in [
        "ExecutionMobility",
        "SystemMobilityExt",
        "InvokerThreadSystem",
    ] {
        assert!(LIB_RS.contains(public), "missing root export: {public}");
        assert!(
            SYSTEM_MOD_RS.contains(public),
            "missing system export: {public}"
        );
    }
    assert!(PRELUDE_RS.contains("SystemMobilityExt"));
    assert!(!PRELUDE_RS.contains("ExecutionMobility"));

    for private in [
        "TransferableSystemRunner",
        "InvokerThreadSystemRunner",
        "RegisteredSystemRunner",
        "InvocationOutcome",
    ] {
        assert!(
            !LIB_RS.contains(private),
            "private runner leaked at root: {private}"
        );
        assert!(
            !has_identifier(SYSTEM_MOD_RS, private),
            "private runner leaked through system module: {private}"
        );
    }
    assert!(SCHEDULER_SYSTEM_RS.contains("TransferableSystemRunner"));
    assert!(SCHEDULER_SYSTEM_RS.contains("+ Send"));
    assert!(SYSTEM_RUNTIME_RS.contains("new_transferable"));
    assert!(SYSTEM_RUNTIME_RS.contains("new_invoker_thread_only"));
    assert!(!SYSTEM_RUNTIME_RS.contains("deferred_commands_ref"));
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

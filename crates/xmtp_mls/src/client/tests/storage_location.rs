//! Client builds that open a storage location: which paths they open, which
//! deployment they record, and what they refuse before any I/O or request.

#[cfg(not(target_arch = "wasm32"))]
mod admission_tests;

#[cfg(not(target_arch = "wasm32"))]
use {
    crate::context::CoreContext,
    crate::storage_location::{
        ResolvedPaths, StorageLocation, StorageLocationError, deployment_component,
        test_support::recorded_deployment,
    },
    std::path::PathBuf,
    std::sync::Arc,
};

#[cfg(not(target_arch = "wasm32"))]
mod native;

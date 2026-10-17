//! Tests for the `group_message` table.

mod aggregates;
mod filters;
mod helpers;
mod queries;
#[cfg(not(target_arch = "wasm32"))]
mod relation_indexes;
mod relations;

pub(crate) use helpers::*;

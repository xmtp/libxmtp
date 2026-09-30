//! The AppData dictionary: component IDs, the component registry, typed
//! component values, validation, and creation-time synthesis.
//!
//! [`fields`] is the developer-facing view: typed metadata fields and user
//! data read from one committed dictionary snapshot.
//! [`typed_facade`] gives protocol code single typed component reads.

pub mod component_id;
pub mod component_permissions;
pub mod component_registry;
pub mod component_source;
pub mod components;
pub mod creation;
pub mod fields;
pub mod migration;
pub mod policy_set;
pub mod protocol_floor;
pub mod registry_table;
pub mod typed;
pub mod typed_facade;
pub mod validation;

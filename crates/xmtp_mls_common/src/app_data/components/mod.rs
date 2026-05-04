//! Concrete [`Component`](super::typed::Component) impls for every
//! well-known XMTP component id. One submodule per family of
//! identically-shaped components (Bytes/String metadata attributes share
//! a module; the heterogeneous Set/Map components each get their own).
//!
//! Adding a new well-known component is two steps:
//!   1. Add a unit-struct impl here under the appropriate submodule.
//!   2. Add it to the `WELL_KNOWN` array in
//!      [`super::registry_table`].

pub mod inbox_id_set;
pub mod metadata_attributes;
pub mod tls_map_components;

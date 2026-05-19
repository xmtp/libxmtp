//! Health-check ops registry.
//!
//! Every op exercises one user-visible libxmtp operation. Each op lives in
//! its own submodule and self-registers via `inventory::submit!` with a
//! `&'static` reference to its (zero-sized) impl. The `registry()` function
//! returns ops topologically sorted by their declared `depends_on`
//! relationships.

use crate::app::health::context::HealthContext;
use crate::app::health::registry::{self, Named, RegistryEntry};
use crate::app::health::result::OpResult;
use async_trait::async_trait;

mod add_members;
mod create_dm;
mod create_group;
mod create_identity;
mod get_mutable_metadata;
mod leave_group;
mod remove_member;
mod send_message;
mod update_admin_list;
mod update_app_data;
mod update_commit_log_signer;
mod update_consent_state;
mod update_group_description;
mod update_group_image_url;
mod update_group_name;
mod update_message_disappearing;
mod update_permission_policy;
mod upload_key_package;

pub mod tree;

#[async_trait]
pub trait HealthOp: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult>;
}

impl Named for dyn HealthOp {
    fn name(&self) -> &'static str {
        HealthOp::name(self)
    }
}

/// Self-registration entry for an op. Each op submits one of these from
/// its own module via `inventory::submit!`. The name lives on the op's own
/// `HealthOp::name()` impl — no duplication on the entry.
pub struct OpEntry {
    /// Names of ops that must complete before this one runs.
    pub depends_on: &'static [&'static str],
    /// `&'static` reference to the op's impl. Ops are zero-sized unit
    /// structs, so `&MyOp` is `'static`-promotable in a `static` context.
    pub op: &'static (dyn HealthOp + Sync),
}

inventory::collect!(OpEntry);

impl RegistryEntry for OpEntry {
    type Item = dyn HealthOp;
    const KIND: &'static str = "op";

    fn depends_on(&self) -> &'static [&'static str] {
        self.depends_on
    }
    fn value(&self) -> &'static dyn HealthOp {
        self.op
    }
}

/// Topologically-sorted list of every registered op.
pub fn registry() -> Vec<&'static dyn HealthOp> {
    registry::topo_sort::<OpEntry>(inventory::iter::<OpEntry>)
}

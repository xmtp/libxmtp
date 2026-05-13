//! Health-check ops registry.
//!
//! Every op exercises one user-visible libxmtp operation. Each op lives in
//! its own submodule and is registered in `registry()` in the spec's
//! prescribed execution order.

use crate::app::health::context::HealthContext;
use crate::app::health::result::OpResult;
use async_trait::async_trait;

mod upload_key_package;
mod create_identity;
mod create_group;
mod add_members;

#[async_trait]
pub trait HealthOp: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: &mut HealthContext) -> Vec<OpResult>;
}

/// Ordered registry of every op in the run.
/// Populated incrementally by Tasks 5–23.
pub fn registry() -> Vec<Box<dyn HealthOp>> {
    vec![
        Box::new(upload_key_package::UploadKeyPackage),
        Box::new(create_identity::CreateIdentity),
        Box::new(create_group::CreateGroup),
        Box::new(add_members::AddMembersToNewGroup),
        Box::new(add_members::AddPrimaryToExistingGroups),
    ]
}

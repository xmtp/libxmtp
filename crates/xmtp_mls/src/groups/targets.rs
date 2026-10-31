//! Membership changes addressed by account or handle targets.

use super::{GroupError, MlsGroup, UpdateGroupMembershipResult};
use crate::context::XmtpSharedContext;
use crate::inbox_handles::{IdentityTarget, resolve_targets_in};
use xmtp_id::InboxId;

/// The inboxes of `targets`, failing with [`GroupError::AddressNotFound`],
/// which names each account with no inbox in request order.
pub(crate) fn require_account_inboxes(
    targets: &[IdentityTarget],
    inboxes: Vec<Option<InboxId>>,
) -> Result<Vec<InboxId>, GroupError> {
    let missing: Vec<String> = targets
        .iter()
        .zip(&inboxes)
        .filter_map(|(target, inbox)| match (target, inbox) {
            (IdentityTarget::Account(identifier), None) => Some(identifier.to_string()),
            _ => None,
        })
        .collect();
    if !missing.is_empty() {
        return Err(GroupError::AddressNotFound(missing));
    }
    Ok(inboxes.into_iter().flatten().collect())
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Add `targets` to the group in one commit. Every target resolves first,
    /// and the add fails before any commit when a target has no inbox: a
    /// handle with [`HandleError::NotFound`], an account with
    /// [`GroupError::AddressNotFound`].
    pub async fn add_members_by_targets(
        &self,
        targets: &[IdentityTarget],
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        let inboxes = resolve_targets_in(&self.context, targets).await?;
        let inbox_ids = require_account_inboxes(targets, inboxes)?;
        self.add_members(&inbox_ids).await
    }

    /// Remove `targets` from the group in one commit. Every target resolves
    /// first. The removal fails before any commit when a handle has no inbox,
    /// and skips an account with no inbox.
    pub async fn remove_members_by_targets(
        &self,
        targets: &[IdentityTarget],
    ) -> Result<(), GroupError> {
        let inbox_ids: Vec<InboxId> = resolve_targets_in(&self.context, targets)
            .await?
            .into_iter()
            .flatten()
            .collect();
        let inbox_ids: Vec<&str> = inbox_ids.iter().map(String::as_str).collect();
        self.remove_members(&inbox_ids).await
    }
}

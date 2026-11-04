//! Membership changes addressed by account or handle targets.

use super::{GroupError, MlsGroup, UpdateGroupMembershipResult};
use crate::groups::context::XmtpSharedContext;
use xmtp_id::InboxId;
use xmtp_mls_identity::identity_updates::IdentityUpdateError;
use xmtp_mls_identity::inbox_handles::{IdentityTarget, resolve_targets_in};

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

/// The inbox of each of `targets`, in order, from one verified lookup. A
/// handle with no inbox fails with
/// [`HandleError::NotFound`](xmtp_mls_identity::inbox_handles::HandleError::NotFound),
/// and accounts with no inbox fail with [`GroupError::AddressNotFound`].
pub async fn target_inboxes<Context, E>(
    context: &Context,
    targets: &[IdentityTarget],
) -> Result<Vec<InboxId>, E>
where
    Context: XmtpSharedContext,
    E: From<IdentityUpdateError> + From<GroupError>,
{
    let inboxes = resolve_targets_in(context, targets).await?;
    Ok(require_account_inboxes(targets, inboxes)?)
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

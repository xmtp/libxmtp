//! Inbox handles on the client.
//!
//! A handle is a short public name that an inbox claims in its identity log
//! (`xmtp_id::associations::handle`). Only the backend answers which inbox
//! holds a handle or an account, so every path from a handle or an account
//! to an inbox goes through one resolver. It looks the target up, loads the
//! answered inbox's identity updates, and keeps the answer only when the
//! newest state the client derived holds the target. The client reports a
//! changed display handle with the
//! `handle.changed` event (`identity_updates::store_association_state`), and
//! group members carry the handles of the newest state the client holds.
//!
//! The client applies no handle policy. The backend alone refuses reserved,
//! short, taken, or too frequent claims, and the client never applies the
//! published minimum name length.

use crate::{
    Client,
    client::ClientError,
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup, UpdateGroupMembershipResult, group_permissions::PolicySet},
    identity_updates::{
        IdentityUpdates, get_association_state_for_client, get_association_state_with_verifier,
        load_identity_updates, load_identity_updates_for_client, state_holds_identifier,
    },
};
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::HashMap;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_configuration::IDENTITY_DEPENDENCY_CONCURRENCY;
use xmtp_db::{NotFound, prelude::*};
use xmtp_id::{
    InboxId,
    associations::{
        AssociationError, AssociationState, Identifier, MAX_HANDLE_CLAIMS,
        handle::{Handle, InvalidHandle},
    },
    scw_verifier::SmartContractSignatureVerifier,
};
use xmtp_mls_common::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
};
use xmtp_proto::{types::ApiIdentifier, xmtp::identity::associations::IdentifierKind};

#[cfg(test)]
mod tests;

/// How many random codes [`Client::available_handles`] checks for a name
/// without a code. With the name itself, one lookup holds 101 handles.
pub const HANDLE_SEARCH_CODES: usize = 100;

/// The number of four-digit codes, `0000` to `9999`.
const HANDLE_CODE_COUNT: usize = 10_000;

/// A handle operation that the client refuses.
#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum HandleError {
    #[error(transparent)]
    #[error_code(inherit)]
    Invalid(#[from] InvalidHandle),
    /// No inbox holds the handle. Either the backend names no inbox for it,
    /// or the inbox it names does not hold the handle in any state the client
    /// derived from that inbox's log. Not retryable.
    #[error("no inbox holds the handle")]
    NotFound,
    /// The inbox has made the most handle claims its identity log can hold,
    /// so a validator rejects another. Not retryable.
    #[error("the inbox has made the most handle claims it can")]
    LimitReached,
}

impl RetryableError for HandleError {
    fn is_retryable(&self) -> bool {
        false
    }
}

/// Someone a caller wants to reach: an account identifier or a handle.
/// Either resolves to an inbox only through the verified resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityTarget {
    Account(Identifier),
    Handle(Handle),
}

impl IdentityTarget {
    /// The lookup of this target in `GetInboxIds`.
    fn lookup(&self) -> ApiIdentifier {
        match self {
            Self::Account(identifier) => identifier.into(),
            Self::Handle(handle) => ApiIdentifier {
                identifier: handle.as_str().to_owned(),
                identifier_kind: IdentifierKind::Handle,
            },
        }
    }
}

/// `handles` as targets, in order.
fn handle_targets(handles: &[Handle]) -> Vec<IdentityTarget> {
    handles
        .iter()
        .cloned()
        .map(IdentityTarget::Handle)
        .collect()
}

/// `accounts` as targets, in order.
pub(crate) fn account_targets(accounts: &[Identifier]) -> Vec<IdentityTarget> {
    accounts
        .iter()
        .cloned()
        .map(IdentityTarget::Account)
        .collect()
}

/// Fail with [`HandleError::LimitReached`] when `state` has made as many
/// claims as one identity log can hold. A validator rejects one more, so the
/// client does not sign it.
pub(crate) fn ensure_claim_allowed(state: &AssociationState) -> Result<(), HandleError> {
    if state.handle_claims() >= MAX_HANDLE_CLAIMS {
        return Err(HandleError::LimitReached);
    }
    Ok(())
}

/// The inbox the backend names for each target, in order, from one request.
/// Nothing here checks an answer.
async fn lookup_targets<A: XmtpApi>(
    api: &ApiClientWrapper<A>,
    targets: &[IdentityTarget],
) -> Result<Vec<Option<InboxId>>, ClientError> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    Ok(api
        .get_inbox_ids(targets.iter().map(IdentityTarget::lookup).collect())
        .await?)
}

/// Each inbox that an answer names, once.
fn answered_inboxes(answers: &[Option<InboxId>]) -> Vec<InboxId> {
    let mut inboxes: Vec<InboxId> = answers.iter().flatten().cloned().collect();
    inboxes.sort_unstable();
    inboxes.dedup();
    inboxes
}

/// An inbox's state, or `None` when its log holds no update. Such an inbox
/// holds no account and no handle.
fn state_if_logged(
    result: Result<AssociationState, ClientError>,
) -> Result<Option<AssociationState>, ClientError> {
    match result {
        Ok(state) => Ok(Some(state)),
        Err(ClientError::Association(AssociationError::MissingIdentityUpdate)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Whether `state`, the newest state of an inbox after this answer's load,
/// holds `target`. Nothing removes a handle, so the newest state holds every
/// handle the inbox ever claimed. A revocation removes an account, so an
/// older state that holds it does not count. An Ethereum address matches in
/// any letter case, and a passkey by its public key bytes.
fn state_holds(state: &AssociationState, target: &IdentityTarget) -> bool {
    match target {
        IdentityTarget::Handle(handle) => state.handles().contains(handle),
        IdentityTarget::Account(identifier) => state_holds_identifier(state, identifier),
    }
}

/// Keep each answer only when the newest state of the inbox it names holds
/// its target.
// implements: IDENT-085, IDENT-096
fn keep_verified(
    targets: &[IdentityTarget],
    answers: Vec<Option<InboxId>>,
    states: &HashMap<InboxId, AssociationState>,
) -> Vec<Option<InboxId>> {
    targets
        .iter()
        .zip(answers)
        .map(|(target, answer)| {
            answer.filter(|inbox_id| {
                states
                    .get(inbox_id)
                    .is_some_and(|state| state_holds(state, target))
            })
        })
        .collect()
}

/// The verified resolver, the only reader of a `GetInboxIds` answer that
/// picks an inbox: look the targets up in one request, load the identity
/// updates of every answered inbox in one incremental batch, derive each
/// inbox's newest state, at most [`IDENTITY_DEPENDENCY_CONCURRENCY`] at once,
/// and drop any answer whose state does not hold its target.
pub(crate) async fn resolve_verified_in<Context: XmtpSharedContext>(
    context: &Context,
    targets: &[IdentityTarget],
) -> Result<Vec<Option<InboxId>>, ClientError> {
    let answers = lookup_targets(context.api(), targets).await?;
    let inboxes = answered_inboxes(&answers);
    if inboxes.is_empty() {
        return Ok(answers);
    }
    let conn = context.db();
    let inbox_refs: Vec<&str> = inboxes.iter().map(String::as_str).collect();
    load_identity_updates_for_client(context, &conn, &inbox_refs).await?;
    // Collected first, as `IdentityUpdates::batch_get_association_state`
    // does, so the future stays `Send` for every caller.
    let reads: Vec<_> = inboxes
        .into_iter()
        .map(|inbox_id| async {
            let state = get_association_state_for_client(context, &conn, &inbox_id, None).await;
            Ok::<_, ClientError>(state_if_logged(state)?.map(|state| (inbox_id, state)))
        })
        .collect();
    let states: Vec<_> = stream::iter(reads)
        .buffered(IDENTITY_DEPENDENCY_CONCURRENCY)
        .try_collect()
        .await?;
    let states: HashMap<InboxId, AssociationState> = states.into_iter().flatten().collect();
    Ok(keep_verified(targets, answers, &states))
}

/// The verified resolver for a caller with no client, such as an SDK static
/// helper. It stores the states it derives, with no event.
pub async fn resolve_verified_with_verifier<A: XmtpApi>(
    api: &ApiClientWrapper<A>,
    conn: &impl DbQuery,
    targets: &[IdentityTarget],
    verifier: &impl SmartContractSignatureVerifier,
) -> Result<Vec<Option<InboxId>>, ClientError> {
    let answers = lookup_targets(api, targets).await?;
    let inboxes = answered_inboxes(&answers);
    if inboxes.is_empty() {
        return Ok(answers);
    }
    let inbox_refs: Vec<&str> = inboxes.iter().map(String::as_str).collect();
    load_identity_updates(api, conn, &inbox_refs).await?;
    let mut states = HashMap::new();
    for inbox_id in inboxes {
        let state = get_association_state_with_verifier(conn, &inbox_id, None, verifier).await;
        if let Some(state) = state_if_logged(state)? {
            states.insert(inbox_id, state);
        }
    }
    Ok(keep_verified(targets, answers, &states))
}

/// The inbox of each target, in order, from one verified lookup. A handle
/// that no inbox holds fails with [`HandleError::NotFound`]. An account with
/// no inbox is `None`, so each route treats it as its account-only path does.
pub(crate) async fn resolve_targets_in<Context: XmtpSharedContext>(
    context: &Context,
    targets: &[IdentityTarget],
) -> Result<Vec<Option<InboxId>>, ClientError> {
    let inboxes = resolve_verified_in(context, targets).await?;
    targets
        .iter()
        .zip(inboxes)
        .map(|(target, inbox)| match (target, inbox) {
            (IdentityTarget::Handle(_), None) => Err(HandleError::NotFound.into()),
            (_, inbox) => Ok(inbox),
        })
        .collect()
}

/// The inboxes of `targets`, failing with [`GroupError::AddressNotFound`],
/// which names each account with no inbox in request order.
fn require_account_inboxes(
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

/// The handles to look up for `handle`: only itself when it has a code.
/// Otherwise itself first, then its name with [`HANDLE_SEARCH_CODES`]
/// distinct random codes.
fn search_candidates(handle: Handle) -> Vec<Handle> {
    if handle.name() != handle.as_str() {
        return vec![handle];
    }
    let codes = rand::seq::index::sample(
        &mut xmtp_common::rng(),
        HANDLE_CODE_COUNT,
        HANDLE_SEARCH_CODES,
    );
    let coded: Vec<Handle> = codes
        .into_iter()
        .map(|code| {
            let code = u16::try_from(code).expect("a sampled code is below 10 000");
            Handle::with_code(handle.name(), code)
                .expect("a name part with a four-digit code is a handle")
        })
        .collect();
    std::iter::once(handle).chain(coded).collect()
}

impl<Context: XmtpSharedContext> Client<Context> {
    /// The inbox that holds each handle, in order, or `None`. An answer from
    /// the backend counts only when the newest state the client derives for
    /// the answered inbox holds the handle. An old handle of an inbox still
    /// resolves to it.
    pub async fn resolve_handles(
        &self,
        handles: &[Handle],
    ) -> Result<Vec<Option<InboxId>>, ClientError> {
        resolve_verified_in(&self.context, &handle_targets(handles)).await
    }

    /// Whether the backend names an inbox for each handle, in order. This
    /// checks no association state: use it only where a wrong answer costs a
    /// candidate, never to reach an inbox.
    pub async fn handles_have_owner(&self, handles: &[Handle]) -> Result<Vec<bool>, ClientError> {
        Ok(lookup_targets(self.context.api(), &handle_targets(handles))
            .await?
            .iter()
            .map(Option::is_some)
            .collect())
    }

    /// The handles for `input` that no inbox holds. `input` is a handle as a
    /// user typed it; its ASCII capitals become lowercase. Input with a code
    /// checks only that handle. A name without a code checks the name and
    /// [`HANDLE_SEARCH_CODES`] distinct random codes in one lookup, and the
    /// name comes first in the result. Text outside the handle grammar fails
    /// before any request. No policy applies here, so a returned handle can
    /// still be refused when it is claimed.
    // implements: CONF-081
    pub async fn available_handles(&self, input: &str) -> Result<Vec<Handle>, ClientError> {
        let handle = Handle::from_input(input).map_err(HandleError::from)?;
        let candidates = search_candidates(handle);
        let owned = self.handles_have_owner(&candidates).await?;
        Ok(candidates
            .into_iter()
            .zip(owned)
            .filter_map(|(candidate, owned)| (!owned).then_some(candidate))
            .collect())
    }

    /// Claim `handle` for this client's inbox, signed by this installation's
    /// key. A claim of a handle the inbox already holds makes it the display
    /// handle again. The backend refuses a taken, reserved, short, or too
    /// frequent claim with `ApiError::HandleRejected`. After the publish, the
    /// client has stored the inbox's new identity update.
    pub async fn claim_handle(&self, handle: &Handle) -> Result<(), ClientError> {
        self.ensure_identity_ready()?;
        let identity_updates = IdentityUpdates::new(&self.context);
        let conn = self.context.db();
        load_identity_updates_for_client(&self.context, &conn, &[self.inbox_id()]).await?;
        let state = identity_updates
            .get_association_state(&conn, self.inbox_id(), None)
            .await?;
        ensure_claim_allowed(&state)?;
        let request = identity_updates
            .claim_handle_request(handle.clone())
            .await?;
        identity_updates.apply_signature_request(request).await
    }

    /// The inbox of each target, in order, from one call. Fails when any
    /// target has no inbox: an account as `find_or_create_dm_by_identity`
    /// does, and a handle with [`HandleError::NotFound`].
    pub async fn resolve_targets(
        &self,
        targets: &[IdentityTarget],
    ) -> Result<Vec<InboxId>, ClientError> {
        let inboxes = resolve_targets_in(&self.context, targets).await?;
        targets
            .iter()
            .zip(inboxes)
            .map(|(target, inbox)| match (target, inbox) {
                (_, Some(inbox)) => Ok(inbox),
                (IdentityTarget::Account(identifier), None) => {
                    Err(NotFound::InboxIdForAddress(identifier.to_string()).into())
                }
                (IdentityTarget::Handle(_), None) => Err(HandleError::NotFound.into()),
            })
            .collect()
    }

    /// The active DM with `target`, or `None`. A target that no inbox holds
    /// has no DM.
    pub async fn find_dm_by_target(
        &self,
        target: &IdentityTarget,
    ) -> Result<Option<MlsGroup<Context>>, ClientError> {
        let Some(peer) = resolve_verified_in(&self.context, std::slice::from_ref(target))
            .await?
            .pop()
            .flatten()
        else {
            return Ok(None);
        };
        let stored = self.context.db().find_active_dm_group(&DmMembers {
            member_one_inbox_id: self.inbox_id(),
            member_two_inbox_id: peer.as_str(),
        })?;
        stored.map(|stored| self.group(&stored.id)).transpose()
    }

    /// Find the DM with `target`, or create it. A handle resolves before the
    /// existing DM is looked up.
    pub async fn find_or_create_dm_by_target(
        &self,
        target: &IdentityTarget,
        opts: DMMetadataOptions,
    ) -> Result<MlsGroup<Context>, ClientError> {
        match target {
            IdentityTarget::Account(identifier) => {
                self.find_or_create_dm_by_identity(identifier.clone(), Some(opts))
                    .await
            }
            IdentityTarget::Handle(handle) => {
                let peer = self
                    .resolve_handles(std::slice::from_ref(handle))
                    .await?
                    .pop()
                    .flatten()
                    .ok_or(HandleError::NotFound)?;
                self.find_or_create_dm(peer, Some(opts)).await
            }
        }
    }

    /// Create a group with `targets` as its members, in one commit. Targets,
    /// member identities, and key packages resolve before the group is stored.
    /// A target with no inbox leaves no group: a handle fails with
    /// [`HandleError::NotFound`], an account with [`GroupError::AddressNotFound`].
    pub async fn create_group_with_targets(
        &self,
        targets: &[IdentityTarget],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let inboxes = resolve_targets_in(&self.context, targets).await?;
        let inbox_ids = require_account_inboxes(targets, inboxes)?;
        self.create_group_with_members(&inbox_ids, permissions_policy_set, opts)
            .await
    }
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

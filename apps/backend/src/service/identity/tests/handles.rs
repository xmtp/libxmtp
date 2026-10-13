//! Handle claims and lookups: admission, the policy only the backend applies,
//! and resolution of old and new handles.

use super::*;
use crate::{config::Config, test_support::RunningServer};
use std::time::Duration;
use support::{
    handles::{ClaimingInbox, resolve},
    locks::{publish_task, wait_for_lock},
};

/// Just over the interval between claims.
const PAST_INTERVAL: Duration = Duration::from_secs(8 * 24 * 60 * 60);

/// The index and reason a refused publish reports.
fn refusal(error: &tonic::Status) -> support::TestResult<(Option<u32>, Reason)> {
    assert_eq!(error.code(), Code::InvalidArgument, "{error:?}");
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    Ok((detail.index, detail.reason()))
}

/// Publish `updates` in one request and return what the refusal reports.
async fn refused(
    server: &RunningServer,
    updates: Vec<IdentityUpdate>,
) -> support::TestResult<(Option<u32>, Reason)> {
    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: updates.into_iter().map(identity_envelope).collect(),
        })
        .await
        .expect_err("the publish is refused");
    refusal(&error)
}

/// A second backend on the same database with `change` applied to the
/// configuration, as an operator restart with a new setting gives.
async fn restarted(
    server: &RunningServer,
    change: impl FnOnce(&mut Config),
) -> support::TestResult<RunningServer> {
    let mut config = (*server.backend.config).clone();
    change(&mut config);
    config.validate()?;
    RunningServer::new(config).await
}

/// The `ClaimHandle` signature bytes of `update`, changed so they no longer
/// verify.
fn with_bad_signature(mut update: IdentityUpdate) -> IdentityUpdate {
    let Some(Kind::ClaimHandle(claim)) = &mut update.actions[0].kind else {
        unreachable!("the update claims a handle")
    };
    let Some(signature::Signature::InstallationKey(key)) = claim
        .signature
        .as_mut()
        .and_then(|signature| signature.signature.as_mut())
    else {
        unreachable!("the installation signs the claim")
    };
    key.bytes[0] ^= 1;
    update
}

/// The rows that hold `handle`, as `(inbox_id, revoked)`.
async fn holders(
    server: &RunningServer,
    handle: &str,
) -> support::TestResult<Vec<(Vec<u8>, bool)>> {
    Ok(sqlx::query_as(
        "SELECT inbox_id, revocation_sequence_id IS NOT NULL FROM identifier_association
         WHERE identifier = $1 AND identifier_kind = 3 ORDER BY inbox_id",
    )
    .bind(handle)
    .fetch_all(&server.backend.store.primary)
    .await?)
}

/// A handle signs nothing, so a creation or a recovery change that names one
/// as its identifier is refused on admission as an invalid identity update,
/// not as a malformed payload, and nothing is stored.
// verifies: IDENT-077
#[xmtp_common::test(unwrap_try = true)]
async fn handle_kind_outside_lookups_is_invalid_identity_update() {
    let server = TestServer::new(|_| {}).await?;
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let inbox_id = wallet.get_inbox_id(0);
    let mut create = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(wallet.identifier(), 0)
        .build();
    add_wallet_signature(&mut create, &wallet).await;
    let create: IdentityUpdate = create.build_identity_update()?.into();
    let mut change = SignatureRequestBuilder::new(&inbox_id)
        .change_recovery_address(wallet.identifier().into(), Identifier::rand_ethereum())
        .build();
    add_wallet_signature(&mut change, &wallet).await;
    let change: IdentityUpdate = change.build_identity_update()?.into();

    let mut handle_creation = create.clone();
    let Some(Kind::CreateInbox(action)) = &mut handle_creation.actions[0].kind else {
        unreachable!("the request creates an inbox")
    };
    action.initial_identifier_kind = IdentifierKind::Handle.into();
    let mut handle_recovery = change.clone();
    let Some(Kind::ChangeRecoveryAddress(action)) = &mut handle_recovery.actions[0].kind else {
        unreachable!("the request changes the recovery identifier")
    };
    action.new_recovery_identifier_kind = IdentifierKind::Handle.into();

    for (case, update, stored) in [
        ("creation", handle_creation, None),
        ("recovery change", handle_recovery, Some(create)),
    ] {
        if let Some(stored) = stored {
            server.publish(vec![identity_envelope(stored)]).await?;
        }
        let before = identity_rows(&server).await?;
        let error = server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![identity_envelope(update)],
            })
            .await
            .expect_err(case);
        assert_eq!(
            rejection(&error)?,
            Reason::InvalidIdentityUpdate,
            "{case}: {error:?}"
        );
        assert_eq!(identity_rows(&server).await?, before, "{case}");
    }
    // The same change with its own kind is admitted.
    server.publish(vec![identity_envelope(change)]).await?;
    server.stop().await?;
}

/// A claimed handle resolves to its inbox, whatever the ASCII letter case of
/// the lookup, and the answer echoes the identifier as sent. A handle no one
/// claimed, the same name without the code included, resolves to no inbox.
// verifies: API-299
#[xmtp_common::test(unwrap_try = true)]
async fn claimed_handle_resolves_by_exact_lookup() {
    let server = TestServer::new(|_| {}).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    inbox.publish_claim(&server, "nick#0042").await?;

    let rows: Vec<(String, i16, Vec<u8>)> = sqlx::query_as(
        "SELECT identifier, identifier_kind, inbox_id FROM identifier_association
         WHERE identifier_kind = 3",
    )
    .fetch_all(&server.backend.store.primary)
    .await?;
    assert_eq!(
        rows,
        [("nick#0042".into(), 3, hex::decode(&inbox.inbox_id)?)]
    );

    let owner = Some(inbox.inbox_id.clone());
    assert_eq!(
        resolve(
            &server,
            &["nick#0042", "NICK#0042", "Nick#0042", "nick", "nick#0043"]
        )
        .await?,
        [owner.clone(), owner.clone(), owner, None, None]
    );
    let answer = server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest {
            requests: vec![api::get_inbox_ids_request::Request {
                identifier: "NICK#0042".into(),
                identifier_kind: IdentifierKind::Handle.into(),
            }],
        })
        .await?
        .into_inner();
    assert_eq!(answer.responses[0].identifier, "NICK#0042");
    assert_eq!(
        answer.responses[0].identifier_kind,
        i32::from(IdentifierKind::Handle)
    );
    server.stop().await?;
}

/// A handle lookup that the grammar does not match after its ASCII letters are
/// lowercased fails the whole request with a fixed message that does not
/// carry the handle.
// verifies: API-299
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_handle_lookup_is_invalid_argument() {
    let server = TestServer::new(|_| {}).await?;
    for input in ["nïck", "nick#12", "_nick", "nick.eth", "nick ", ""] {
        let error = resolve(&server, &["nick", input]).await.expect_err(input);
        assert_eq!(error.code(), Code::InvalidArgument, "{input:?}");
        assert_eq!(error.message(), "malformed handle", "{input:?}");
    }
    server.stop().await?;
}

/// A claim of a handle another inbox holds fails at its own index, behind a
/// valid update at a lower index, and stores nothing.
// verifies: API-210, API-223, API-296
#[xmtp_common::test(unwrap_try = true)]
async fn taken_handle_is_rejected_at_its_index() {
    let server = TestServer::new(|_| {}).await?;
    let owner = ClaimingInbox::registered(&server).await?;
    owner.publish_claim(&server, "nick").await?;
    let second = ClaimingInbox::registered(&server).await?;
    let bystander = identity_history_with_passkey().await;
    let before = identity_rows(&server).await?;

    let claim = second.claim("nick");
    assert_eq!(
        refused(&server, vec![bystander.history[0].clone(), claim]).await?,
        (Some(1), Reason::HandleTaken)
    );
    assert_eq!(identity_rows(&server).await?, before);
    assert_eq!(
        resolve(&server, &["nick"]).await?,
        [Some(owner.inbox_id.clone())]
    );
    server.stop().await?;
}

/// Two inboxes that claim one handle in one request: the lower index wins
/// the check, the later index is taken, and nothing is stored.
// verifies: API-296
#[xmtp_common::test(unwrap_try = true)]
async fn same_request_claims_of_one_handle_fail_at_the_later_index() {
    let server = TestServer::new(|_| {}).await?;
    let first = ClaimingInbox::registered(&server).await?;
    let second = ClaimingInbox::registered(&server).await?;
    let before = identity_rows(&server).await?;

    let claims = vec![first.claim("nick"), second.claim("nick")];
    assert_eq!(
        refused(&server, claims).await?,
        (Some(1), Reason::HandleTaken)
    );
    assert_eq!(identity_rows(&server).await?, before);
    assert_eq!(resolve(&server, &["nick"]).await?, [None]);
    server.stop().await?;
}

/// The lowest failing index is reported with its own reason. A taken claim
/// whose signature does not verify fails as an invalid signature, not as
/// taken, and a taken claim below a bad signature fails as taken.
// verifies: API-223
#[xmtp_common::test(unwrap_try = true)]
async fn handle_rejection_reports_lowest_failing_index() {
    let server = TestServer::new(|_| {}).await?;
    let owner = ClaimingInbox::registered(&server).await?;
    owner.publish_claim(&server, "nick").await?;
    let second = ClaimingInbox::registered(&server).await?;
    let third = ClaimingInbox::registered(&server).await?;

    let bad_taken = with_bad_signature(second.claim("nick"));
    let taken = third.claim("nick");
    assert_eq!(
        refused(&server, vec![bad_taken, taken.clone()]).await?,
        (Some(0), Reason::InvalidSignature)
    );
    let bad_free = with_bad_signature(second.claim("other"));
    assert_eq!(
        refused(&server, vec![taken, bad_free]).await?,
        (Some(0), Reason::HandleTaken)
    );
    server.stop().await?;
}

/// One claim that fails several checks reports the first in this order: the
/// validator, then the name, then the interval, then taken.
// verifies: API-223, API-296, API-297, API-298
#[xmtp_common::test(unwrap_try = true)]
async fn reason_order_follows_pd3() {
    let server = TestServer::new(|_| {}).await?;
    let owner = ClaimingInbox::registered(&server).await?;
    owner.publish_claim(&server, "nick").await?;
    // A reserved name the owner held before the name was reserved.
    sqlx::query(
        "INSERT INTO identifier_association
            (identifier, identifier_kind, inbox_id, association_sequence_id)
         VALUES ('admin', 3, $1, 1)",
    )
    .bind(hex::decode(&owner.inbox_id)?)
    .execute(&server.backend.store.primary)
    .await?;
    let recent = ClaimingInbox::registered(&server).await?;
    recent.publish_claim(&server, "bobby").await?;
    let fresh = ClaimingInbox::registered(&server).await?;

    // Too soon and taken.
    assert_eq!(
        refused(&server, vec![recent.claim("nick")]).await?,
        (Some(0), Reason::HandleTooSoon)
    );
    // Reserved and taken.
    assert_eq!(
        refused(&server, vec![fresh.claim("admin")]).await?,
        (Some(0), Reason::HandleNotAllowed)
    );
    // Reserved and too soon.
    assert_eq!(
        refused(&server, vec![recent.claim("admin")]).await?,
        (Some(0), Reason::HandleNotAllowed)
    );
    // A signer that is not a member, a reserved name, and taken.
    assert_eq!(
        refused(&server, vec![fresh.claim_by_stranger("admin")]).await?,
        (Some(0), Reason::InvalidIdentityUpdate)
    );
    server.stop().await?;
}

/// Two requests that claim one handle at once: the second validates before
/// the first commits, then waits for the identity lock. The taken check under
/// the lock sees the first claim, so one claim is stored and the other fails
/// as taken, not as an internal error.
// verifies: API-296
#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_claims_of_one_handle_store_one() {
    let server = TestServer::new(|_| {}).await?;
    let first = ClaimingInbox::registered(&server).await?;
    let second = ClaimingInbox::registered(&server).await?;
    let pool = &server.backend.store.primary;
    let mut blocker = pool.begin().await?;
    sqlx::query!("LOCK TABLE envelopes IN SHARE MODE")
        .execute(&mut *blocker)
        .await?;

    let winner = publish_task(&server, identity_envelope(first.claim("nick")));
    wait_for_lock(pool, "relation").await?;
    let loser = publish_task(&server, identity_envelope(second.claim("nick")));
    wait_for_lock(pool, "advisory").await?;
    blocker.commit().await?;

    winner.await??;
    let error = loser.await?.expect_err("the second claim is refused");
    assert_eq!(refusal(&error)?, (Some(0), Reason::HandleTaken));
    assert_eq!(
        holders(&server, "nick").await?,
        [(hex::decode(&first.inbox_id)?, false)]
    );
    server.stop().await?;
}

/// A reserved name part is refused with or without a code; a longer name that
/// starts with one is admitted.
// verifies: API-297
#[xmtp_common::test(unwrap_try = true)]
async fn reserved_names_and_codes_are_not_allowed() {
    let server = TestServer::new(|_| {}).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    for handle in ["admin", "admin#0001", "support#1234", "xmtp"] {
        assert_eq!(
            refused(&server, vec![inbox.claim(handle)]).await?,
            (Some(0), Reason::HandleNotAllowed),
            "{handle}"
        );
    }
    inbox.publish_claim(&server, "adminx").await?;
    assert_eq!(
        resolve(&server, &["adminx", "admin"]).await?,
        [Some(inbox.inbox_id.clone()), None]
    );
    server.stop().await?;
}

/// A name part shorter than the configured minimum is refused. After the
/// operator raises the minimum, a handle claimed before the change still
/// resolves, and a new claim must meet the new minimum.
// verifies: API-297, CONF-069
#[xmtp_common::test(unwrap_try = true)]
async fn short_name_is_not_allowed_and_existing_handles_survive_a_raise() {
    let server = TestServer::new(|_| {}).await?;
    let first = ClaimingInbox::registered(&server).await?;
    assert_eq!(
        refused(&server, vec![first.claim("ab")]).await?,
        (Some(0), Reason::HandleNotAllowed)
    );
    first.publish_claim(&server, "abc#0001").await?;

    let mut raised = restarted(&server, |config| config.handles.min_name_length = 5).await?;
    assert_eq!(
        resolve(&raised, &["abc#0001"]).await?,
        [Some(first.inbox_id.clone())]
    );
    let second = ClaimingInbox::registered(&raised).await?;
    assert_eq!(
        refused(&raised, vec![second.claim("abcd")]).await?,
        (Some(0), Reason::HandleNotAllowed)
    );
    second.publish_claim(&raised, "abcde").await?;
    raised.stop().await?;
    server.stop().await?;
}

/// A switch back to a handle the inbox already holds is a new claim, so it
/// runs every check. Under a raised minimum the old handle cannot be selected
/// again, yet it still resolves to the inbox.
// verifies: API-297
#[xmtp_common::test(unwrap_try = true)]
async fn switch_back_runs_policy() {
    let server = TestServer::new(|_| {}).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    inbox.publish_claim(&server, "abcd").await?;
    server
        .age_identity_history(&inbox.inbox_id, PAST_INTERVAL)
        .await?;
    inbox.publish_claim(&server, "abcdefg").await?;

    let mut raised = restarted(&server, |config| config.handles.min_name_length = 5).await?;
    raised
        .age_identity_history(&inbox.inbox_id, PAST_INTERVAL)
        .await?;
    assert_eq!(
        refused(&raised, vec![inbox.claim("abcd")]).await?,
        (Some(0), Reason::HandleNotAllowed)
    );
    assert_eq!(
        resolve(&raised, &["abcd", "abcdefg"]).await?,
        [Some(inbox.inbox_id.clone()), Some(inbox.inbox_id.clone())]
    );
    raised.stop().await?;
    server.stop().await?;
}

/// The interval is measured in server time from the inbox's last stored
/// claim: a claim a minute short of 7 days is too soon, one a second past 7
/// days is not. The unit tests pin the exact boundary.
// verifies: API-298
#[xmtp_common::test(unwrap_try = true)]
async fn second_claim_inside_interval_is_too_soon() {
    let server = TestServer::new(|_| {}).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    inbox.publish_claim(&server, "first").await?;
    let interval = Duration::from_nanos(crate::handles::CLAIM_INTERVAL_NS as u64);
    // Wide enough that the time this test takes cannot cross the boundary.
    let margin = Duration::from_secs(60);

    server
        .age_identity_history(&inbox.inbox_id, interval - margin)
        .await?;
    assert_eq!(
        refused(&server, vec![inbox.claim("second")]).await?,
        (Some(0), Reason::HandleTooSoon)
    );
    server
        .age_identity_history(&inbox.inbox_id, margin + Duration::from_secs(1))
        .await?;
    inbox.publish_claim(&server, "second").await?;
    server.stop().await?;
}

/// A claim sent again after its response was lost returns the stored
/// metadata, even a day later, when a new claim would be too soon.
// verifies: API-222
#[xmtp_common::test(unwrap_try = true)]
async fn retried_claim_is_a_duplicate_not_a_rejection() {
    let server = TestServer::new(|_| {}).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    let claim = identity_envelope(inbox.claim("nick"));
    let stored = server.publish(vec![claim.clone()]).await?;
    server
        .age_identity_history(&inbox.inbox_id, Duration::from_secs(24 * 60 * 60))
        .await?;
    let retried = server.publish(vec![claim]).await?;
    assert_eq!(
        retried[0].cursor.as_ref()?.sequence_id,
        stored[0].cursor.as_ref()?.sequence_id
    );
    assert_eq!(holders(&server, "nick").await?.len(), 1);
    server.stop().await?;
}

/// After a switch, the old handle and the new one both resolve to the inbox,
/// and no handle row is revoked.
// verifies: API-299
#[xmtp_common::test(unwrap_try = true)]
async fn old_handle_still_resolves_after_switch() {
    let server = TestServer::new(|_| {}).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    inbox.publish_claim(&server, "first").await?;
    server
        .age_identity_history(&inbox.inbox_id, PAST_INTERVAL)
        .await?;
    inbox.publish_claim(&server, "second").await?;

    let owner = Some(inbox.inbox_id.clone());
    assert_eq!(
        resolve(&server, &["first", "second"]).await?,
        [owner.clone(), owner]
    );
    for handle in ["first", "second"] {
        assert_eq!(
            holders(&server, handle).await?,
            [(hex::decode(&inbox.inbox_id)?, false)],
            "{handle}"
        );
    }
    server.stop().await?;
}

/// A stored claim that the current policy would refuse still replays: after
/// the minimum rises above a stored handle's length, a later update of the
/// same inbox is admitted.
// verifies: IDENT-084
#[xmtp_common::test(unwrap_try = true)]
async fn stored_claims_outside_policy_still_replay() {
    let server = TestServer::new(|config| config.handles.min_name_length = 1).await?;
    let inbox = ClaimingInbox::registered(&server).await?;
    inbox.publish_claim(&server, "ab").await?;

    let mut raised = restarted(&server, |config| config.handles.min_name_length = 5).await?;
    raised
        .publish(vec![identity_envelope(inbox.add_wallet().await)])
        .await?;
    raised.stop().await?;
    server.stop().await?;
}

/// Each handle rejection is counted under its own reason label.
// verifies: OPS-017
#[xmtp_common::test(unwrap_try = true)]
async fn handle_rejections_use_their_reason_label() {
    let Some(metrics) = support::metrics::isolated(
        "service::identity::tests::handles::handle_rejections_use_their_reason_label",
    ) else {
        return;
    };
    let server = TestServer::new(|_| {}).await?;
    let owner = ClaimingInbox::registered(&server).await?;
    owner.publish_claim(&server, "nick").await?;
    let other = ClaimingInbox::registered(&server).await?;
    for (inbox, handle, reason) in [
        (&other, "nick", Reason::HandleTaken),
        (&other, "admin", Reason::HandleNotAllowed),
        (&owner, "later", Reason::HandleTooSoon),
    ] {
        assert_eq!(
            refused(&server, vec![inbox.claim(handle)]).await?,
            (Some(0), reason)
        );
    }
    for label in [
        "REASON_HANDLE_TAKEN",
        "REASON_HANDLE_NOT_ALLOWED",
        "REASON_HANDLE_TOO_SOON",
    ] {
        assert_eq!(
            support::metrics::value(
                &metrics,
                "xmtp_publish_rejections_total",
                &[("reason", label)]
            ),
            1.0,
            "{label}"
        );
    }
    server.stop().await?;
}

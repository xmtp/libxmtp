//! The verified handle resolver.

use super::{random_handle, scripted_client, wait_until_claim_visible};
use crate::{context::XmtpSharedContext, identity::IdentityExt, tester};
use xmtp_db::prelude::*;
use xmtp_id::associations::{Action, MemberIdentifier};

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn claim_then_resolve_returns_own_inbox() {
    tester!(alix);
    tester!(bo);
    let handle = random_handle();
    alix.claim_handle(&handle).await?;

    // The claim stored alix's new update, signed by the installation key.
    let stored = alix
        .context
        .db()
        .get_identity_updates(alix.inbox_id(), None, None)?;
    let newest = stored
        .into_iter()
        .last()
        .expect("the claim is stored")
        .to_unverified()?
        .to_verified(&alix.context.scw_verifier())
        .await?;
    let installation = MemberIdentifier::installation(alix.installation_public_key().to_vec());
    assert!(matches!(&newest.actions[..], [Action::ClaimHandle(claim)]
        if claim.handle == handle.as_str() && claim.signature.signer == installation));

    let unclaimed = random_handle();
    wait_until_claim_visible(&bo, &handle).await;
    assert_eq!(
        bo.resolve_handles(&[handle, unclaimed]).await?,
        vec![Some(alix.inbox_id().to_owned()), None]
    );
}

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn resolver_drops_answer_whose_state_lacks_the_handle() {
    tester!(alix, disable_workers);
    tester!(bo);
    let (client, script) = scripted_client(&alix).await?;
    let handle = random_handle();
    // The backend names bo for a handle bo never claimed.
    script
        .lock()
        .lies
        .insert(handle.to_string(), bo.inbox_id().to_owned());

    assert_eq!(client.resolve_handles(&[handle]).await?, vec![None]);
}

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn resolver_loads_updates_newer_than_the_cache() {
    tester!(alix);
    tester!(bo);
    // Bo holds alix's state from before the claim.
    let before = bo.inbox_addresses(true, vec![alix.inbox_id()]).await?;
    assert!(before[0].handles().is_empty());
    let handle = random_handle();
    alix.claim_handle(&handle).await?;
    // The lookup checks no state, so waiting on it leaves bo's cache stale.
    wait_until_claim_visible(&bo, &handle).await;

    assert_eq!(
        bo.resolve_handles(&[handle]).await?,
        vec![Some(alix.inbox_id().to_owned())]
    );
}

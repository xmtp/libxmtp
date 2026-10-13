//! Handle tests. Tests on the shared backend use random handles and claim at
//! most once per inbox, because a claimed handle is permanent there. Tests
//! that claim more than once use an ephemeral backend.

mod events;
#[cfg(not(target_arch = "wasm32"))]
mod members;

use crate::{
    client::ClientError, context::XmtpSharedContext, identity_updates::IdentityUpdates,
    utils::Tester,
};
use xmtp_id::associations::{
    AssociationState, Identifier,
    builder::SignatureRequestBuilder,
    handle::Handle,
    test_utils::{WalletTestExt, add_installation_key_signature, add_wallet_signature},
};

/// Longer than the backend's interval of 7 days between two claims by one
/// inbox. Aging an inbox's identity history by it admits the inbox's next claim.
#[cfg(not(target_arch = "wasm32"))]
const PAST_CLAIM_INTERVAL: std::time::Duration = std::time::Duration::from_secs(8 * 24 * 60 * 60);

/// A handle that no other test run claims.
fn random_handle() -> Handle {
    let suffix = hex::encode(xmtp_common::rand_vec::<10>());
    Handle::parse(&format!("t{suffix}")).expect("a random handle matches the grammar")
}

/// The current member that signs a test claim.
#[derive(Clone, Copy)]
enum ClaimSigner {
    Installation,
    /// The tester's wallet. The signature text has one-second precision, so a
    /// switch back to a handle in the same second by the same signer repeats
    /// that signer's earlier signature, which replay protection rejects.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    Wallet,
}

/// Claim `handle` for the tester's inbox.
async fn claim_for_test(
    tester: &Tester,
    handle: &Handle,
    signer: ClaimSigner,
) -> Result<(), ClientError> {
    let wallet = &tester.builder.owner;
    let installation_keys = &tester.context.identity().installation_keys;
    let member = match signer {
        ClaimSigner::Installation => installation_keys.verifying_key().into(),
        ClaimSigner::Wallet => wallet.member_identifier(),
    };
    let mut request = SignatureRequestBuilder::new(tester.inbox_id())
        .claim_handle(handle.clone(), member)
        .build();
    match signer {
        ClaimSigner::Installation => {
            add_installation_key_signature(&mut request, installation_keys).await
        }
        ClaimSigner::Wallet => add_wallet_signature(&mut request, wallet).await,
    }
    IdentityUpdates::new(&tester.context)
        .apply_signature_request(request)
        .await
}

/// `base` after claims of `handles`, in order. It is built in memory, for
/// tests that store states directly.
fn state_with_handles(base: &AssociationState, handles: &[&str]) -> AssociationState {
    handles.iter().fold(base.clone(), |state, handle| {
        state.claim_handle(Handle::parse(handle).expect("test handles match the grammar"))
    })
}

/// A new inbox's association state, with no handles.
fn new_inbox_state() -> AssociationState {
    AssociationState::new(Identifier::rand_ethereum(), 0, None).expect("a new inbox state")
}

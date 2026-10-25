//! Applying `ClaimHandle` actions: what a claim changes, who may sign one,
//! and how many one log may hold.

use super::{
    Action, AddAssociation, AssociationError, AssociationState, ChangeRecoveryIdentity,
    ClaimHandle, CreateInbox, Handle, IdentityUpdate, MAX_HANDLE_CLAIMS, MemberIdentifier,
    MemberKind, RevokeAssociation, SignatureKind, apply_update,
    builder::SignatureRequestBuilder,
    get_state,
    member::Identifier,
    test_utils::{
        MockSmartContractSignatureVerifier, WalletTestExt, add_wallet_signature,
        ecdsa_recovery_byte_alias,
    },
    tests::{new_test_inbox, new_test_inbox_with_installation},
    unverified::{UnverifiedAction, UnverifiedIdentityUpdate, UnverifiedSignature},
    verified_signature::VerifiedSignature,
};
use alloy::signers::local::PrivateKeySigner;
use xmtp_common::rand_vec;

/// A signature by `signer` of `kind`, with a fresh replay key.
fn signed_by(signer: &MemberIdentifier, kind: SignatureKind) -> VerifiedSignature {
    VerifiedSignature::new(signer.clone(), kind, rand_vec::<32>(), None)
}

/// An update to `state`'s inbox that claims `handle`, signed by `signer`.
fn claim(
    state: &AssociationState,
    handle: &str,
    signer: &MemberIdentifier,
    kind: SignatureKind,
) -> IdentityUpdate {
    IdentityUpdate::new_test(
        vec![Action::ClaimHandle(ClaimHandle {
            handle: handle.to_string(),
            signature: signed_by(signer, kind),
        })],
        state.inbox_id().to_string(),
    )
}

/// An update to `state`'s inbox that carries only `action`.
fn update(state: &AssociationState, action: Action) -> IdentityUpdate {
    IdentityUpdate::new_test(vec![action], state.inbox_id().to_string())
}

/// `state` after its installation claims each of `handles` in turn.
fn claimed_by_installation(mut state: AssociationState, handles: &[&str]) -> AssociationState {
    let installation = installation(&state);
    for handle in handles {
        let claim = claim(
            &state,
            handle,
            &installation,
            SignatureKind::InstallationKey,
        );
        state = apply_update(state, claim).expect("a member's claim applies");
    }
    state
}

fn installation(state: &AssociationState) -> MemberIdentifier {
    state.members_by_kind(MemberKind::Installation)[0]
        .identifier
        .clone()
}

fn wallet(state: &AssociationState) -> MemberIdentifier {
    state.recovery_identifier().clone().into()
}

fn handles(state: &AssociationState) -> Vec<&str> {
    state.handles().iter().map(Handle::as_str).collect()
}

fn other_handles(state: &AssociationState) -> Vec<&str> {
    state.other_handles().map(Handle::as_str).collect()
}

/// A claim adds its handle at the end and counts once. Members, the recovery
/// identifier, and chain ids stay as they were.
// verifies: IDENT-082
#[xmtp_common::test(unwrap_try = true)]
fn installation_claim_appends_handle() {
    let state = new_test_inbox_with_installation();
    let claimed = claimed_by_installation(state.clone(), &["nick"]);
    assert_eq!(handles(&claimed), ["nick"]);
    assert_eq!(claimed.display_handle()?.as_str(), "nick");
    assert!(other_handles(&claimed).is_empty());
    assert_eq!(claimed.handle_claims(), 1);

    assert_eq!(claimed.members().len(), state.members().len());
    for member in state.members() {
        assert_eq!(claimed.get(&member.identifier), Some(&member));
    }
    assert_eq!(claimed.recovery_identifier(), state.recovery_identifier());

    let second = claimed_by_installation(claimed, &["nick#0001"]);
    assert_eq!(handles(&second), ["nick", "nick#0001"]);
    assert_eq!(second.display_handle()?.as_str(), "nick#0001");
    assert_eq!(other_handles(&second), ["nick"]);
    assert_eq!(second.handle_claims(), 2);
}

/// A claim of a handle the inbox already holds moves it to the end, so the
/// user goes back to it, and it still counts as a claim.
// verifies: IDENT-082
#[xmtp_common::test(unwrap_try = true)]
fn switch_back_moves_handle_and_counts() {
    let state = claimed_by_installation(new_test_inbox_with_installation(), &["a", "b", "a"]);
    assert_eq!(handles(&state), ["b", "a"]);
    assert_eq!(state.display_handle()?.as_str(), "a");
    assert_eq!(other_handles(&state), ["b"]);
    assert_eq!(state.handle_claims(), 3);
}

/// The log holds 64 claims, switches back included, and no more: a 65th
/// claim fails whether or not the inbox already holds its handle.
// verifies: IDENT-080
#[xmtp_common::test(unwrap_try = true)]
fn sixty_fifth_claim_is_rejected() {
    let alternating: Vec<&str> = (0..MAX_HANDLE_CLAIMS)
        .map(|n| if n % 2 == 0 { "nick" } else { "nick#0001" })
        .collect();
    let full = claimed_by_installation(new_test_inbox_with_installation(), &alternating);
    assert_eq!(full.handle_claims(), MAX_HANDLE_CLAIMS);
    assert_eq!(handles(&full), ["nick", "nick#0001"]);

    let installation = installation(&full);
    for handle in ["nick#0002", "nick"] {
        let result = apply_update(
            full.clone(),
            claim(&full, handle, &installation, SignatureKind::InstallationKey),
        );
        assert!(
            matches!(result, Err(AssociationError::HandleClaimLimit)),
            "{handle}: {result:?}"
        );
    }
}

/// Every current member may sign a claim: an installation, an Ethereum
/// address, or a passkey. No one else may: not a stranger, not a revoked
/// installation, and not a recovery identifier that is no longer a member.
// verifies: IDENT-081
#[xmtp_common::test(unwrap_try = true)]
fn only_current_members_sign_claims() {
    let state = new_test_inbox_with_installation();
    let wallet = wallet(&state);
    let installation = installation(&state);
    let passkey: MemberIdentifier = Identifier::passkey(rand_vec::<33>(), None).into();
    let state = apply_update(
        state.clone(),
        update(
            &state,
            Action::AddAssociation(AddAssociation {
                new_member_identifier: passkey.clone(),
                new_member_signature: signed_by(&passkey, SignatureKind::P256),
                existing_member_signature: signed_by(&wallet, SignatureKind::Erc191),
            }),
        ),
    )?;

    for (signer, kind) in [
        (&installation, SignatureKind::InstallationKey),
        (&wallet, SignatureKind::Erc191),
        (&passkey, SignatureKind::P256),
    ] {
        let claimed = apply_update(state.clone(), claim(&state, "nick", signer, kind))?;
        assert_eq!(handles(&claimed), ["nick"]);
    }

    let rejected = |state: &AssociationState, signer: &MemberIdentifier, kind| {
        let result = apply_update(state.clone(), claim(state, "nick", signer, kind));
        assert!(
            matches!(result, Err(AssociationError::MissingExistingMember)),
            "{signer:?}: {result:?}"
        );
    };
    rejected(
        &state,
        &MemberIdentifier::rand_ethereum(),
        SignatureKind::Erc191,
    );
    rejected(
        &state,
        &MemberIdentifier::rand_installation(),
        SignatureKind::InstallationKey,
    );

    let revoke = |member: &MemberIdentifier| {
        Action::RevokeAssociation(RevokeAssociation {
            recovery_identifier_signature: signed_by(&wallet, SignatureKind::Erc191),
            revoked_member: member.clone(),
        })
    };
    let without_installation = apply_update(state.clone(), update(&state, revoke(&installation)))?;
    rejected(
        &without_installation,
        &installation,
        SignatureKind::InstallationKey,
    );

    // The recovery identifier revokes itself as a member and keeps its role.
    let without_wallet = apply_update(state.clone(), update(&state, revoke(&wallet)))?;
    assert!(without_wallet.get(&wallet).is_none());
    assert_eq!(&wallet, without_wallet.recovery_identifier());
    rejected(&without_wallet, &wallet, SignatureKind::Erc191);
}

/// A claim's signature is used once. An old claim published again, to move
/// an older handle back without a new signature, is a replay, in its own
/// encoding or in another encoding of the same signature.
// verifies: IDENT-090
#[xmtp_common::test(unwrap_try = true)]
async fn claim_signature_is_used_once() {
    let wallet = PrivateKeySigner::random();
    let inbox_id = wallet.get_inbox_id(0);
    let verifier = MockSmartContractSignatureVerifier::new(false);
    let signed = |builder: SignatureRequestBuilder| async {
        let mut request = builder.build();
        add_wallet_signature(&mut request, &wallet).await;
        request.build_identity_update().unwrap()
    };
    let claim_of = |handle: &str| {
        SignatureRequestBuilder::new(&inbox_id)
            .claim_handle(Handle::parse(handle).unwrap(), wallet.member_identifier())
    };

    let create =
        signed(SignatureRequestBuilder::new(&inbox_id).create_inbox(wallet.identifier(), 0))
            .await
            .to_verified(&verifier)
            .await?;
    let claim_a = signed(claim_of("nick")).await;
    let claim_b = signed(claim_of("nick#0001")).await;
    let state = get_state([create])?;
    let state = apply_update(state, claim_a.to_verified(&verifier).await?)?;
    let state = apply_update(state, claim_b.to_verified(&verifier).await?)?;
    assert_eq!(state.display_handle()?.as_str(), "nick#0001");

    let mut aliased: UnverifiedIdentityUpdate = claim_a.clone();
    let [UnverifiedAction::ClaimHandle(action)] = &mut aliased.actions[..] else {
        unreachable!("the request holds one claim")
    };
    let UnverifiedSignature::RecoverableEcdsa(signature) = &mut action.signature else {
        unreachable!("the wallet signs with ERC-191")
    };
    signature.signature_bytes = ecdsa_recovery_byte_alias(&signature.signature_bytes);
    assert_ne!(aliased, claim_a);

    for replay in [claim_a, aliased] {
        let result = apply_update(state.clone(), replay.to_verified(&verifier).await?);
        assert!(
            matches!(result, Err(AssociationError::Replay)),
            "{result:?}"
        );
    }
}

/// A smart contract wallet member signs a claim on the chain it was added on,
/// and on no other.
// verifies: IDENT-046
#[xmtp_common::test(unwrap_try = true)]
fn claim_signer_keeps_its_chain() {
    let signer = Identifier::rand_ethereum();
    let member: MemberIdentifier = signer.clone().into();
    let scw = |chain_id| {
        VerifiedSignature::new(
            member.clone(),
            SignatureKind::Erc1271,
            rand_vec::<32>(),
            Some(chain_id),
        )
    };
    let state = get_state([IdentityUpdate::new_test(
        vec![Action::CreateInbox(CreateInbox {
            nonce: 0,
            account_identifier: signer.clone(),
            initial_identifier_signature: scw(1),
        })],
        signer.inbox_id(0)?,
    )])?;
    let claim_on = |chain_id| {
        update(
            &state,
            Action::ClaimHandle(ClaimHandle {
                handle: "nick".into(),
                signature: scw(chain_id),
            }),
        )
    };

    let result = apply_update(state.clone(), claim_on(2));
    assert!(
        matches!(result, Err(AssociationError::ChainIdMismatch(1, 2))),
        "{result:?}"
    );
    assert_eq!(
        handles(&apply_update(state.clone(), claim_on(1))?),
        ["nick"]
    );
}

/// Only a claim changes the handles and the claim count. A creation starts
/// with none. An add, a change of recovery identifier, and a revocation, even
/// of the member that signed the claims, leave both as they were.
// verifies: IDENT-083
#[xmtp_common::test(unwrap_try = true)]
fn other_actions_keep_handles_and_claim_count() {
    let created = new_test_inbox();
    assert!(created.handles().is_empty());
    assert_eq!(created.handle_claims(), 0);
    assert!(created.display_handle().is_none());

    let state = claimed_by_installation(new_test_inbox_with_installation(), &["a", "b"]);
    let wallet = wallet(&state);
    let installation = installation(&state);
    let new_recovery = Identifier::rand_ethereum();
    let new_wallet: MemberIdentifier = new_recovery.clone().into();
    let actions = [
        Action::AddAssociation(AddAssociation {
            new_member_identifier: new_wallet.clone(),
            new_member_signature: signed_by(&new_wallet, SignatureKind::Erc191),
            existing_member_signature: signed_by(&wallet, SignatureKind::Erc191),
        }),
        Action::ChangeRecoveryIdentity(ChangeRecoveryIdentity {
            recovery_identifier_signature: signed_by(&wallet, SignatureKind::Erc191),
            new_recovery_identifier: new_recovery,
        }),
        Action::RevokeAssociation(RevokeAssociation {
            recovery_identifier_signature: signed_by(&new_wallet, SignatureKind::Erc191),
            revoked_member: installation.clone(),
        }),
        Action::RevokeAssociation(RevokeAssociation {
            recovery_identifier_signature: signed_by(&new_wallet, SignatureKind::Erc191),
            revoked_member: wallet,
        }),
    ];

    let mut current = state;
    for action in actions {
        current = apply_update(current.clone(), update(&current, action))?;
        assert_eq!(handles(&current), ["a", "b"]);
        assert_eq!(current.handle_claims(), 2);
    }
    assert!(current.get(&installation).is_none());
}

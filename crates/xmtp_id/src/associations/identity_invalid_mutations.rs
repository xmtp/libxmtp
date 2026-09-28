//! Identity updates that a validator must reject before they change any
//! association state or reach a chain verifier.

use super::{
    AccountId, Action, AddAssociation, AssociationError, IdentityUpdate, MemberIdentifier,
    MemberKind, RevokeAssociation, SignatureKind, apply_update, get_state,
    tests::new_test_inbox_with_installation,
    unverified::{UnverifiedCreateInbox, UnverifiedIdentityUpdate, UnverifiedSignature},
    verified_signature::VerifiedSignature,
};
use crate::associations::unsigned_actions::UnsignedCreateInbox;
use crate::associations::unverified::UnverifiedAction;
use crate::associations::{
    Identifier,
    test_utils::{
        LEGACY_WALLET, identity_update, length_delimited, retired_signature,
        retired_signature_create_inbox,
    },
    tests::new_test_inbox,
};
use crate::scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError};
use alloy::primitives::{BlockNumber, Bytes};
use prost::Message;
use std::sync::atomic::{AtomicUsize, Ordering};
use xmtp_common::{RetryableError, rand_hexstring, rand_vec};
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::associations::{
    AddAssociation as AddProto, ChangeRecoveryAddress as ChangeRecoveryProto,
    CreateInbox as CreateInboxProto, IdentityAction as ActionProto,
    IdentityUpdate as IdentityUpdateProto, MemberIdentifier as MemberIdentifierProto,
    RecoverableEcdsaSignature, RevokeAssociation as RevokeProto, Signature, identity_action::Kind,
    member_identifier, signature,
};

fn erc191() -> Signature {
    Signature {
        signature: Some(signature::Signature::Erc191(RecoverableEcdsaSignature {
            bytes: rand_vec::<65>(),
        })),
    }
}

fn eth(address: &str) -> Option<MemberIdentifierProto> {
    Some(MemberIdentifierProto {
        kind: Some(member_identifier::Kind::EthereumAddress(
            address.to_string(),
        )),
    })
}

fn decode(bytes: &[u8]) -> Result<UnverifiedIdentityUpdate, ConversionError> {
    IdentityUpdateProto::decode(bytes)?.try_into()
}

/// Every action that carries an Ethereum identifier, with `address` in that
/// identifier and every other field well formed.
fn actions_carrying(address: &str) -> [Kind; 4] {
    [
        Kind::CreateInbox(CreateInboxProto {
            initial_identifier: address.to_string(),
            initial_identifier_signature: Some(erc191()),
            ..Default::default()
        }),
        Kind::Add(AddProto {
            new_member_identifier: eth(address),
            existing_member_signature: Some(erc191()),
            new_member_signature: Some(erc191()),
            relying_party: None,
        }),
        Kind::Revoke(RevokeProto {
            member_to_revoke: eth(address),
            recovery_identifier_signature: Some(erc191()),
        }),
        Kind::ChangeRecoveryAddress(ChangeRecoveryProto {
            new_recovery_identifier: address.to_string(),
            existing_recovery_identifier_signature: Some(erc191()),
            ..Default::default()
        }),
    ]
}

/// Counts chain calls, and would accept every signature it is asked about.
#[derive(Default)]
struct CountingVerifier(AtomicUsize);

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for CountingVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        _signature: Bytes,
        _block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(ValidationResponse {
            is_valid: true,
            block_number: Some(1),
            error: None,
        })
    }
}

/// An update with no actions carries no signature, so every signature rule
/// passes it. Rejecting it keeps an inbox's finite log from filling with
/// updates that change nothing and so blocking a later revocation.
// verifies: IDENT-001
#[xmtp_common::test(unwrap_try = true)]
fn empty_update_is_rejected() {
    let state = new_test_inbox();
    let inbox_id = state.inbox_id().to_string();
    let empty = IdentityUpdate::new_test(vec![], inbox_id.clone());

    assert!(matches!(
        apply_update(state.clone(), empty.clone()),
        Err(AssociationError::EmptyUpdate)
    ));
    // The wire form decodes, so rejection must come from validation.
    let decoded = decode(&identity_update(&inbox_id, vec![]))?;
    assert!(decoded.actions.is_empty());
    assert!(matches!(
        get_state(vec![empty.clone()]),
        Err(AssociationError::EmptyUpdate)
    ));
}

/// A signer is always derived as `0x` and 40 lowercase hex characters, so an
/// Ethereum identifier in any other form never matches its own signature; a
/// recovery identifier stored that way can never recover. Every identifier
/// position is checked when the update is decoded, before any signature is
/// verified, and the identifier is rejected rather than rewritten.
// verifies: IDENT-013
#[xmtp_common::test(unwrap_try = true)]
fn noncanonical_ethereum_identifier_is_rejected() {
    let canonical = rand_hexstring();
    let hex = canonical.trim_start_matches("0x");
    let malformed = [
        format!("0x{}", hex.to_uppercase()),
        format!("0x{}A", &hex[..39]),
        format!("0X{hex}"),
        hex.to_string(),
        format!("0x{}", &hex[..39]),
        format!("{canonical}0"),
        format!("0x{}g", &hex[..39]),
        String::new(),
    ];

    for kind in actions_carrying(&canonical) {
        let action = ActionProto { kind: Some(kind) }.encode_to_vec();
        decode(&identity_update("inbox", vec![action]))?;
    }
    for address in &malformed {
        for kind in actions_carrying(address) {
            let action = ActionProto { kind: Some(kind) }.encode_to_vec();
            let result = decode(&identity_update("inbox", vec![action]));
            assert!(
                matches!(result, Err(ConversionError::InvalidValue { .. })),
                "{address:?} accepted: {result:?}"
            );
        }
    }
}

/// Adding a current member would overwrite its adder, time, and chain id,
/// and spend a log slot on an update that grants nothing. A member that was
/// revoked may be added again.
// verifies: IDENT-040
#[xmtp_common::test(unwrap_try = true)]
fn add_of_current_member_is_rejected() {
    let state = new_test_inbox_with_installation();
    let inbox_id = state.inbox_id().to_string();
    let wallet: MemberIdentifier = state.recovery_identifier().clone().into();
    let installation = state.members_by_kind(MemberKind::Installation)[0].clone();
    let signature = |signer: &MemberIdentifier, kind| {
        VerifiedSignature::new(signer.clone(), kind, rand_vec::<32>(), None)
    };
    let add = |new: &MemberIdentifier, new_kind, existing: &MemberIdentifier, existing_kind| {
        IdentityUpdate::new_test(
            vec![Action::AddAssociation(AddAssociation {
                new_member_identifier: new.clone(),
                new_member_signature: signature(new, new_kind),
                existing_member_signature: signature(existing, existing_kind),
            })],
            inbox_id.clone(),
        )
    };

    let readd_installation = add(
        &installation.identifier,
        SignatureKind::InstallationKey,
        &wallet,
        SignatureKind::Erc191,
    );
    let readd_wallet = add(
        &wallet,
        SignatureKind::Erc191,
        &installation.identifier,
        SignatureKind::InstallationKey,
    );
    for update in [readd_installation.clone(), readd_wallet] {
        assert!(matches!(
            apply_update(state.clone(), update),
            Err(AssociationError::AlreadyMember)
        ));
    }

    let revoke = IdentityUpdate::new_test(
        vec![Action::RevokeAssociation(RevokeAssociation {
            recovery_identifier_signature: signature(&wallet, SignatureKind::Erc191),
            revoked_member: installation.identifier.clone(),
        })],
        inbox_id.clone(),
    );
    let revoked = apply_update(state, revoke)?;
    let readded = apply_update(revoked, readd_installation)?;
    assert_eq!(
        readded.get(&installation.identifier)?.added_by_entity,
        Some(wallet)
    );
}

/// A smart contract wallet signature names its chain in a CAIP-10 account id.
/// One outside the `eip155` namespace, or whose reference is not a chain id
/// in canonical decimal form, can never be verified, so it is rejected
/// permanently and before any chain call; routing it would instead fail as
/// a retryable missing route and keep it pending for ever.
// verifies: IDENT-060
#[xmtp_common::test(unwrap_try = true)]
async fn non_eip155_account_id_is_rejected_without_chain_call() {
    let verifier = CountingVerifier::default();
    let account = rand_hexstring();
    let update = |chain_id: &str| {
        let identifier = Identifier::eth(&account).unwrap();
        UnverifiedIdentityUpdate::new_test(
            vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                unsigned_action: UnsignedCreateInbox {
                    nonce: 0,
                    account_identifier: identifier.clone(),
                },
                initial_identifier_signature: UnverifiedSignature::new_smart_contract_wallet(
                    rand_vec::<65>(),
                    AccountId::new(chain_id.to_string(), account.clone()),
                    1,
                ),
            })],
            identifier.inbox_id(0).unwrap(),
        )
    };

    for chain_id in [
        "cosmos:cosmoshub-4",
        "eip155",
        "eip155:",
        "eip155:01",
        "eip155:+1",
        "eip155:0x1",
        "eip155:18446744073709551616",
        "EIP155:1",
    ] {
        let err = update(chain_id)
            .to_verified(&verifier)
            .await
            .expect_err(chain_id);
        assert!(!err.is_retryable(), "{chain_id}: {err}");
    }
    assert_eq!(verifier.0.load(Ordering::SeqCst), 0);

    update("eip155:1").to_verified(&verifier).await?;
    assert_eq!(verifier.0.load(Ordering::SeqCst), 1);
}

/// Legacy delegated signatures are retired and field 4 of `Signature` is
/// reserved. An update carrying it must fail validation, on admission and
/// when a stored log is replayed, rather than decode as an update with some
/// other or no signature. The cross-inbox case is the fixture an earlier rule
/// let through: a genuine legacy key associated with an inbox other than its
/// own nonce-0 inbox.
// verifies: IDENT-001
#[xmtp_common::test(unwrap_try = true)]
fn retired_legacy_signature_is_rejected() {
    let cross_inbox_add = [
        AddProto {
            new_member_identifier: eth(LEGACY_WALLET),
            existing_member_signature: Some(erc191()),
            ..Default::default()
        }
        .encode_to_vec(),
        length_delimited(3, &retired_signature()),
    ]
    .concat();
    let own_inbox = Identifier::eth(LEGACY_WALLET)?.inbox_id(0)?;
    let other_inbox = Identifier::rand_ethereum().inbox_id(0)?;

    for bytes in [
        retired_signature_create_inbox(&own_inbox, LEGACY_WALLET),
        identity_update(&other_inbox, vec![length_delimited(2, &cross_inbox_add)]),
    ] {
        // Replay decodes each stored update the same way admission does.
        let result = decode(&bytes);
        assert!(
            matches!(
                result,
                Err(ConversionError::Missing {
                    item: "signature",
                    ..
                })
            ),
            "{result:?}"
        );
    }

    // Beside a listed signature, field 4 is an unknown field like any other:
    // it is ignored, and the listed signature is the one verified.
    let mixed = [
        CreateInboxProto {
            initial_identifier: LEGACY_WALLET.to_string(),
            ..Default::default()
        }
        .encode_to_vec(),
        length_delimited(3, &[erc191().encode_to_vec(), retired_signature()].concat()),
    ]
    .concat();
    let decoded = decode(&identity_update(
        &own_inbox,
        vec![length_delimited(1, &mixed)],
    ))?;
    assert!(matches!(
        &decoded.actions[..],
        [UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
            initial_identifier_signature: UnverifiedSignature::RecoverableEcdsa(_),
            ..
        })]
    ));
}

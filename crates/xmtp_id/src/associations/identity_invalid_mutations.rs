//! Identity updates that a validator must reject before they change any
//! association state or reach a chain verifier.

use super::{
    AccountId, Action, AddAssociation, AssociationError, ClaimHandle, CreateInbox, IdentityUpdate,
    MemberIdentifier, MemberKind, RevokeAssociation, SignatureKind, apply_update, get_state,
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
use xmtp_configuration::MAX_SCW_SIGNATURE_BYTES;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::associations::{
    AddAssociation as AddProto, ChangeRecoveryAddress as ChangeRecoveryProto,
    ClaimHandle as ClaimHandleProto, CreateInbox as CreateInboxProto, IdentifierKind,
    IdentityAction as ActionProto, IdentityUpdate as IdentityUpdateProto,
    MemberIdentifier as MemberIdentifierProto, RecoverableEcdsaSignature,
    RevokeAssociation as RevokeProto, Signature, identity_action::Kind, member_identifier,
    signature,
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
/// One outside the `eip155` namespace, whose reference is not a chain id in
/// canonical decimal form, or whose address is not `0x` and 40 hexadecimal
/// characters or is all zero, can never be verified, and neither can a
/// signature longer than the cap, so each is rejected
/// permanently and before any chain call; routing it would instead fail as
/// a retryable missing route and keep it pending for ever.
// verifies: IDENT-093
#[xmtp_common::test(unwrap_try = true)]
async fn non_eip155_account_id_is_rejected_without_chain_call() {
    let verifier = CountingVerifier::default();
    let account = rand_hexstring();
    let update_signed = |chain_id: &str, address: &str, signature: Vec<u8>| {
        let identifier = Identifier::eth(&account).unwrap();
        UnverifiedIdentityUpdate::new_test(
            vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox {
                unsigned_action: UnsignedCreateInbox {
                    nonce: 0,
                    account_identifier: identifier.clone(),
                },
                initial_identifier_signature: UnverifiedSignature::new_smart_contract_wallet(
                    signature,
                    AccountId::new(chain_id.to_string(), address.to_string()),
                    1,
                ),
            })],
            identifier.inbox_id(0).unwrap(),
        )
    };
    let update = |chain_id: &str, address: &str| update_signed(chain_id, address, rand_vec::<65>());

    let oversized = update_signed("eip155:1", &account, vec![0; MAX_SCW_SIGNATURE_BYTES + 1])
        .to_verified(&verifier)
        .await
        .unwrap_err();
    assert!(!oversized.is_retryable(), "{oversized}");
    for (chain_id, address) in [
        "cosmos:cosmoshub-4",
        "eip155",
        "eip155:",
        "eip155:01",
        "eip155:+1",
        "eip155:0x1",
        "eip155:18446744073709551616",
        "EIP155:1",
    ]
    .map(|chain_id| (chain_id, account.clone()))
    .into_iter()
    .chain(
        [
            "bad".to_string(),
            account.trim_start_matches("0x").to_string(),
            format!("{account}0"),
            format!("0x{}g", &account[3..]),
            format!("0x{}", "0".repeat(40)),
            String::new(),
        ]
        .map(|address| ("eip155:1", address)),
    ) {
        let err = update(chain_id, &address)
            .to_verified(&verifier)
            .await
            .expect_err(&address);
        assert!(!err.is_retryable(), "{chain_id} {address}: {err}");
    }
    assert_eq!(verifier.0.load(Ordering::SeqCst), 0);

    update("eip155:1", &account).to_verified(&verifier).await?;
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

/// An `Action::ClaimHandle` of `handle` signed by `signer`.
fn claim(handle: &str, signer: &MemberIdentifier, kind: SignatureKind) -> Action {
    Action::ClaimHandle(ClaimHandle {
        handle: handle.to_string(),
        signature: VerifiedSignature::new(signer.clone(), kind, rand_vec::<32>(), None),
    })
}

/// A claim carries no other action, whatever the order and whatever the
/// other action is, so one update passes the backend's interval between
/// claims for one claim only. The whole update fails, and the state the
/// caller holds stays as it was.
// verifies: IDENT-002, IDENT-079
#[xmtp_common::test(unwrap_try = true)]
fn claim_with_another_action_is_rejected() {
    let state = new_test_inbox_with_installation();
    let inbox_id = state.inbox_id().to_string();
    let wallet: MemberIdentifier = state.recovery_identifier().clone().into();
    let installation = state.members_by_kind(MemberKind::Installation)[0]
        .identifier
        .clone();
    let installation_claim = |handle| claim(handle, &installation, SignatureKind::InstallationKey);
    let add = || {
        Action::AddAssociation(AddAssociation {
            existing_member_signature: VerifiedSignature::new(
                wallet.clone(),
                SignatureKind::Erc191,
                rand_vec::<32>(),
                None,
            ),
            ..Default::default()
        })
    };

    for actions in [
        vec![installation_claim("nick"), add()],
        vec![add(), installation_claim("nick")],
        vec![installation_claim("nick"), installation_claim("nick#0001")],
    ] {
        let result = apply_update(
            state.clone(),
            IdentityUpdate::new_test(actions, inbox_id.clone()),
        );
        assert!(
            matches!(result, Err(AssociationError::ClaimNotAlone)),
            "{result:?}"
        );
    }
    assert!(state.handles().is_empty());
    assert_eq!(state.handle_claims(), 0);

    // An inbox exists before it claims: a claim beside its creation fails.
    let create = CreateInbox::default();
    let creator: MemberIdentifier = create.account_identifier.clone().into();
    let created_inbox = create.account_identifier.inbox_id(create.nonce)?;
    let result = get_state([IdentityUpdate::new_test(
        vec![
            Action::CreateInbox(create),
            claim("nick", &creator, SignatureKind::Erc191),
        ],
        created_inbox,
    )]);
    assert!(
        matches!(result, Err(AssociationError::ClaimNotAlone)),
        "{result:?}"
    );

    // Each action alone applies, so the pair is what fails.
    let claimed = apply_update(
        state.clone(),
        IdentityUpdate::new_test(vec![installation_claim("nick")], inbox_id.clone()),
    )?;
    assert_eq!(claimed.handle_claims(), 1);
    apply_update(state, IdentityUpdate::new_test(vec![add()], inbox_id))?;
}

/// A claim outside the grammar is rejected when it is applied, and is never
/// rewritten: `Nick` is not read as `nick`. The wire form decodes with the
/// handle as carried, so the rejection is the validator's, not the decoder's.
// verifies: IDENT-078
#[xmtp_common::test(unwrap_try = true)]
fn noncanonical_claim_is_rejected() {
    let state = new_test_inbox_with_installation();
    let inbox_id = state.inbox_id().to_string();
    let installation = state.members_by_kind(MemberKind::Installation)[0]
        .identifier
        .clone();
    for handle in ["Nick", "NICK#0001", "nick ", "nick#123", "1nick", ""] {
        let result = apply_update(
            state.clone(),
            IdentityUpdate::new_test(
                vec![claim(handle, &installation, SignatureKind::InstallationKey)],
                inbox_id.clone(),
            ),
        );
        assert!(
            matches!(result, Err(AssociationError::InvalidHandle(_))),
            "{handle:?}: {result:?}"
        );
    }

    let action = ActionProto {
        kind: Some(Kind::ClaimHandle(ClaimHandleProto {
            handle: "Nick".into(),
            signature: Some(erc191()),
        })),
    }
    .encode_to_vec();
    let decoded = decode(&identity_update(&inbox_id, vec![action]))?;
    let [UnverifiedAction::ClaimHandle(claim)] = &decoded.actions[..] else {
        panic!("the update decodes to one claim: {decoded:?}");
    };
    assert_eq!(claim.unsigned_action.handle, "Nick");
}

/// A handle signs nothing, so it is never an initial or a recovery identifier.
/// A kind `IdentifierKind` does not list is rejected too, never read as
/// Ethereum. The listed signing kinds still decode.
// verifies: IDENT-077
#[xmtp_common::test(unwrap_try = true)]
fn handle_kind_is_rejected_outside_lookups() {
    let address = rand_hexstring();
    let actions = |kind: i32| {
        [
            Kind::CreateInbox(CreateInboxProto {
                initial_identifier: address.clone(),
                initial_identifier_signature: Some(erc191()),
                initial_identifier_kind: kind,
                ..Default::default()
            }),
            Kind::ChangeRecoveryAddress(ChangeRecoveryProto {
                new_recovery_identifier: address.clone(),
                existing_recovery_identifier_signature: Some(erc191()),
                new_recovery_identifier_kind: kind,
                ..Default::default()
            }),
        ]
    };
    let decode_action = |kind| {
        let action = ActionProto { kind: Some(kind) }.encode_to_vec();
        decode(&identity_update("inbox", vec![action]))
    };

    for kind in [IdentifierKind::Handle as i32, 4, -1] {
        for action in actions(kind) {
            let result = decode_action(action);
            assert!(
                matches!(result, Err(ConversionError::NonSigningIdentifierKind(k)) if k == kind),
                "{kind}: {result:?}"
            );
        }
    }
    for kind in [IdentifierKind::Unspecified, IdentifierKind::Ethereum] {
        for action in actions(kind as i32) {
            decode_action(action)?;
        }
    }
}

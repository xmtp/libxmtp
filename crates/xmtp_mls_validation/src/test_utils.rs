//! Stateless payload fixtures for native and wasm callers.

use openmls::{
    key_packages::Lifetime,
    prelude::{
        Ciphersuite, CredentialWithKey, GroupId as OpenMlsGroupId, KeyPackage as OpenMlsKeyPackage,
        LeafNodeParameters, MlsGroup, MlsGroupCreateConfig, MlsMessageOut, OpenMlsCrypto,
        OpenMlsProvider, SignatureScheme, tls_codec::Serialize,
    },
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use xmtp_cryptography::{Secret, XmtpInstallationCredential, utils::generate_local_wallet};
use xmtp_id::{
    InboxOwner,
    associations::{
        AccountId, Identifier, MemberIdentifier,
        builder::SignatureRequestBuilder,
        test_utils::{MockSmartContractSignatureVerifier, WalletTestExt, add_wallet_signature},
        unsigned_actions::UnsignedCreateInbox,
        unverified::{
            UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate, UnverifiedSignature,
        },
    },
    key_package::{KeyPackageOptions, build_key_package, create_credential},
    utils::passkey::PasskeyUser,
};
use xmtp_proto::xmtp::{
    backend::v1::{
        ClientEnvelope, CommitLogEntry, GroupMessage, KeyPackage, WelcomeMessage,
        client_envelope::Payload,
        welcome_message::{V1, Version, WelcomePointer},
    },
    identity::associations::{IdentifierKind, IdentityUpdate},
    mls::message_contents::PlaintextCommitLogEntry,
};

pub const GROUP_ID: [u8; 16] = [0x11; 16];
pub const INSTALLATION_ID: [u8; 32] = [0x22; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMessageKind {
    Application,
    Proposal,
    Commit,
}

/// Build a structurally valid MLS protocol message without a client or database.
pub fn group_message_envelope(
    group_id: impl AsRef<[u8]>,
    kind: GroupMessageKind,
    trailing_bytes: impl AsRef<[u8]>,
) -> ClientEnvelope {
    let group_id = group_id.as_ref();
    let provider = OpenMlsRustCrypto::default();
    let signer = XmtpInstallationCredential::new();
    let credential = CredentialWithKey {
        credential: create_credential("group-message-fixture"),
        signature_key: signer.public_slice().into(),
    };
    let config = MlsGroupCreateConfig::test_default(xmtp_configuration::CIPHERSUITE);
    let mut group = MlsGroup::new_with_group_id(
        &provider,
        &signer,
        &config,
        OpenMlsGroupId::from_slice(group_id),
        credential,
    )
    .expect("group-message fixture creates an in-memory group");
    let message = match kind {
        GroupMessageKind::Application => group
            .create_message(&provider, &signer, b"fixture application")
            .expect("application fixture builds"),
        GroupMessageKind::Proposal => {
            group
                .propose_self_update(&provider, &signer, LeafNodeParameters::default())
                .expect("proposal fixture builds")
                .0
        }
        GroupMessageKind::Commit => {
            group
                .self_update(&provider, &signer, LeafNodeParameters::default())
                .expect("commit fixture builds")
                .into_messages()
                .0
        }
    };
    let mut bytes = message
        .to_bytes()
        .expect("group-message fixture serializes");
    bytes.extend_from_slice(trailing_bytes.as_ref());
    ClientEnvelope {
        payload: Some(Payload::GroupMessage(GroupMessage {
            data: bytes,
            sender_hmac: vec![0x33, 0x44],
            should_push: true,
        })),
    }
}

/// Put a valid MLS key-package body in the group-message field.
pub fn non_protocol_group_message_envelope() -> ClientEnvelope {
    let provider = OpenMlsRustCrypto::default();
    let installation_keys = XmtpInstallationCredential::new();
    let generated = build_key_package(
        "non-protocol-message",
        create_credential("non-protocol-message"),
        &installation_keys,
        &provider,
        KeyPackageOptions::default(),
    )
    .expect("non-protocol fixture key package builds");
    let bytes = MlsMessageOut::from(generated.bundle)
        .to_bytes()
        .expect("non-protocol fixture serializes");
    ClientEnvelope {
        payload: Some(Payload::GroupMessage(GroupMessage {
            data: bytes,
            sender_hmac: vec![],
            should_push: false,
        })),
    }
}

pub fn inline_welcome_envelope(installation_key: impl AsRef<[u8]>) -> ClientEnvelope {
    ClientEnvelope {
        payload: Some(Payload::WelcomeMessage(WelcomeMessage {
            version: Some(Version::V1(V1 {
                installation_key: installation_key.as_ref().to_vec(),
                data: vec![0x10, 0x11],
                hpke_public_key: vec![0x12, 0x13],
                wrapper_algorithm: 1,
                welcome_metadata: vec![0x14, 0x15],
            })),
        })),
    }
}

pub fn welcome_pointer_envelope(installation_key: impl AsRef<[u8]>) -> ClientEnvelope {
    ClientEnvelope {
        payload: Some(Payload::WelcomeMessage(WelcomeMessage {
            version: Some(Version::WelcomePointer(WelcomePointer {
                installation_key: installation_key.as_ref().to_vec(),
                welcome_pointer: vec![0x20, 0x21],
                hpke_public_key: vec![0x22, 0x23],
                wrapper_algorithm: 1,
            })),
        })),
    }
}

#[derive(Debug, Clone)]
pub struct KeyPackageFixture {
    pub envelope: ClientEnvelope,
    pub tls_bytes: Vec<u8>,
    pub installation_id: Vec<u8>,
    pub inbox_id: String,
}

/// Build a valid key package with caller-selected construction options.
pub fn key_package_envelope(
    inbox_id: impl Into<String>,
    options: KeyPackageOptions,
) -> KeyPackageFixture {
    let inbox_id = inbox_id.into();
    let provider = OpenMlsRustCrypto::default();
    let installation_keys = XmtpInstallationCredential::new();
    let generated = build_key_package(
        &inbox_id,
        create_credential(&inbox_id),
        &installation_keys,
        &provider,
        options,
    )
    .expect("key-package fixture builds");
    let tls_bytes = generated
        .bundle
        .key_package()
        .tls_serialize_detached()
        .expect("key-package fixture serializes");
    let envelope = ClientEnvelope {
        payload: Some(Payload::KeyPackage(KeyPackage {
            key_package_tls_serialized: tls_bytes.clone(),
        })),
    };
    KeyPackageFixture {
        envelope,
        tls_bytes,
        installation_id: installation_keys.public_slice().to_vec(),
        inbox_id,
    }
}

/// Build a valid minimal package without XMTP-specific extensions or capabilities.
pub fn minimal_key_package_envelope(
    inbox_id: impl Into<String>,
    ciphersuite: Ciphersuite,
    lifetime: Option<Lifetime>,
) -> KeyPackageFixture {
    let inbox_id = inbox_id.into();
    let provider = OpenMlsRustCrypto::default();
    let installation_keys = XmtpInstallationCredential::new();
    let mut builder = OpenMlsKeyPackage::builder();
    if let Some(lifetime) = lifetime {
        builder = builder.key_package_lifetime(lifetime);
    }
    let bundle = builder
        .build(
            ciphersuite,
            &provider,
            &installation_keys,
            CredentialWithKey {
                credential: create_credential(&inbox_id),
                signature_key: installation_keys.public_slice().into(),
            },
        )
        .expect("minimal key-package fixture builds");
    let tls_bytes = bundle
        .key_package()
        .tls_serialize_detached()
        .expect("minimal key-package fixture serializes");
    KeyPackageFixture {
        envelope: ClientEnvelope {
            payload: Some(Payload::KeyPackage(KeyPackage {
                key_package_tls_serialized: tls_bytes.clone(),
            })),
        },
        tls_bytes,
        installation_id: installation_keys.public_slice().to_vec(),
        inbox_id,
    }
}

pub fn expired_key_package_envelope() -> KeyPackageFixture {
    key_package_envelope(
        "expired-key-package",
        KeyPackageOptions {
            lifetime: Some(Lifetime::init(1, 2)),
            ..Default::default()
        },
    )
}

/// Build a package whose leaf claims a different signature key.
pub fn mismatched_key_package_envelope() -> KeyPackageFixture {
    let inbox_id = "mismatched-key-package".to_string();
    let provider = OpenMlsRustCrypto::default();
    let signer = XmtpInstallationCredential::new();
    let claimed_signer = XmtpInstallationCredential::new();
    let bundle = OpenMlsKeyPackage::builder()
        .build(
            xmtp_configuration::CIPHERSUITE,
            &provider,
            &signer,
            CredentialWithKey {
                credential: create_credential(&inbox_id),
                signature_key: claimed_signer.public_slice().into(),
            },
        )
        .expect("mismatched key-package fixture builds");
    let tls_bytes = bundle
        .key_package()
        .tls_serialize_detached()
        .expect("mismatched key-package fixture serializes");
    KeyPackageFixture {
        envelope: ClientEnvelope {
            payload: Some(Payload::KeyPackage(KeyPackage {
                key_package_tls_serialized: tls_bytes.clone(),
            })),
        },
        tls_bytes,
        installation_id: claimed_signer.public_slice().to_vec(),
        inbox_id,
    }
}

#[derive(Debug, Clone)]
pub struct IdentityHistoryFixture {
    pub inbox_id: String,
    pub history: Vec<IdentityUpdate>,
    pub update: IdentityUpdate,
    pub added_identifier: MemberIdentifier,
}

/// Build a real ECDSA inbox history and a real P-256 passkey association.
pub async fn identity_history_with_passkey() -> IdentityHistoryFixture {
    let recovery = generate_local_wallet();
    let recovery_identifier = recovery.identifier();
    let inbox_id = recovery.get_inbox_id(0);
    let verifier = MockSmartContractSignatureVerifier::new(false);

    let mut create = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(recovery_identifier.clone(), 0)
        .build();
    add_wallet_signature(&mut create, &recovery).await;
    let create: IdentityUpdate = create
        .build_identity_update()
        .expect("signed create update is complete")
        .into();

    let passkey = PasskeyUser::new().await;
    let passkey_identifier = passkey.identifier();
    let added_identifier: MemberIdentifier = passkey_identifier.clone().into();
    let mut add = SignatureRequestBuilder::new(&inbox_id)
        .add_association(added_identifier.clone(), recovery_identifier.into())
        .build();
    add_wallet_signature(&mut add, &recovery).await;
    let passkey_signature = passkey
        .sign(&add.signature_text())
        .expect("passkey fixture signs the association text");
    add.add_signature(passkey_signature, &verifier)
        .await
        .expect("passkey association signature verifies");
    let update = add
        .build_identity_update()
        .expect("signed passkey update is complete")
        .into();

    IdentityHistoryFixture {
        inbox_id,
        history: vec![create],
        update,
        added_identifier,
    }
}

/// Build an update that reaches a smart-contract verifier during signature validation.
pub fn scw_create_inbox_update() -> IdentityUpdate {
    let account = "0x1111111111111111111111111111111111111111";
    let identifier = Identifier::eth(account).expect("fixture account is valid");
    let inbox_id = identifier
        .inbox_id(0)
        .expect("fixture account derives an inbox");
    UnverifiedIdentityUpdate::new(
        inbox_id,
        1,
        vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
            UnsignedCreateInbox {
                account_identifier: identifier,
                nonce: 0,
            },
            UnverifiedSignature::new_smart_contract_wallet(
                vec![0x55],
                AccountId::new_evm(1, account.to_string()),
                1,
            ),
        ))],
    )
    .into()
}

pub fn malformed_signature_create_inbox_update() -> IdentityUpdate {
    let account = "0x2222222222222222222222222222222222222222";
    let identifier = Identifier::eth(account).expect("fixture account is valid");
    let inbox_id = identifier
        .inbox_id(0)
        .expect("fixture account derives an inbox");
    UnverifiedIdentityUpdate::new(
        inbox_id,
        1,
        vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
            UnsignedCreateInbox {
                account_identifier: identifier,
                nonce: 0,
            },
            UnverifiedSignature::new_recoverable_ecdsa(vec![1, 2, 3]),
        ))],
    )
    .into()
}

pub fn identity_envelope(update: IdentityUpdate) -> ClientEnvelope {
    ClientEnvelope {
        payload: Some(Payload::IdentityUpdate(update)),
    }
}

#[derive(Debug, Clone)]
pub struct RawRecoveryFixture {
    pub history: Vec<IdentityUpdate>,
    pub rejected_update: IdentityUpdate,
    pub raw_recovery_identifier: String,
}

/// Preserve the current raw recovery-identifier behavior from SEC-024.
pub async fn identity_history_with_raw_recovery() -> RawRecoveryFixture {
    let original_recovery = generate_local_wallet();
    let next_recovery = generate_local_wallet();
    let original_identifier = original_recovery.identifier();
    let next_identifier = next_recovery.identifier();
    let inbox_id = original_recovery.get_inbox_id(0);

    let mut create = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(original_identifier.clone(), 0)
        .build();
    add_wallet_signature(&mut create, &original_recovery).await;
    let create = create
        .build_identity_update()
        .expect("signed create update is complete")
        .into();

    let raw_recovery_identifier = next_identifier.to_string().to_ascii_uppercase();
    let raw_recovery =
        Identifier::from_proto(&raw_recovery_identifier, IdentifierKind::Ethereum, None)
            .expect("raw Ethereum recovery identifier decodes without normalization");
    let mut change = SignatureRequestBuilder::new(&inbox_id)
        .change_recovery_address(original_identifier.clone().into(), raw_recovery)
        .build();
    add_wallet_signature(&mut change, &original_recovery).await;
    let change = change
        .build_identity_update()
        .expect("signed recovery update is complete")
        .into();

    let mut revoke = SignatureRequestBuilder::new(&inbox_id)
        .revoke_association(next_identifier.clone().into(), original_identifier.into())
        .build();
    add_wallet_signature(&mut revoke, &next_recovery).await;
    let rejected_update = revoke
        .build_identity_update()
        .expect("signed revoke update is complete")
        .into();

    RawRecoveryFixture {
        history: vec![create, change],
        rejected_update,
        raw_recovery_identifier,
    }
}

pub fn commit_log_envelope(group_id: impl AsRef<[u8]>) -> ClientEnvelope {
    let provider = OpenMlsRustCrypto::default();
    let entry = PlaintextCommitLogEntry {
        group_id: group_id.as_ref().to_vec(),
        commit_sequence_id: 7,
        last_epoch_authenticator: vec![0x60, 0x61],
        commit_result: 1,
        applied_epoch_number: 8,
        applied_epoch_authenticator: vec![0x62, 0x63],
    };
    let (private_key, _) = provider
        .crypto()
        .signature_key_gen(SignatureScheme::ED25519)
        .expect("commit-log fixture key generation succeeds");
    let signed = crate::sign_commit_log(&entry, &Secret::new(private_key), provider.crypto())
        .expect("commit-log fixture signing succeeds");
    ClientEnvelope {
        payload: Some(Payload::CommitLogEntry(CommitLogEntry {
            serialized_commit_log_entry: signed.serialized_commit_log_entry,
            signature: Some(signed.signature),
        })),
    }
}

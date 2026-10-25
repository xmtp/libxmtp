//! Stateless payload fixtures for native and wasm callers.

use openmls::{
    extensions::{
        AppDataDictionary, AppDataDictionaryExtension, Extension, ExtensionType, Extensions,
    },
    key_packages::Lifetime,
    prelude::{
        Capabilities, Ciphersuite, CredentialWithKey, GroupId as OpenMlsGroupId,
        KeyPackage as OpenMlsKeyPackage, LeafNodeParameters, MlsGroup, MlsGroupCreateConfig,
        MlsMessageOut, tls_codec::Serialize,
    },
};
use openmls_rust_crypto::OpenMlsRustCrypto;
use prost::Message as _;
use xmtp_cryptography::{XmtpInstallationCredential, utils::generate_local_wallet};
use xmtp_id::{
    InboxOwner,
    associations::{
        AccountId, Identifier, MemberIdentifier,
        builder::SignatureRequestBuilder,
        test_utils::{MockSmartContractSignatureVerifier, WalletTestExt, add_wallet_signature},
        unsigned_actions::{
            UnsignedChangeRecoveryAddress, UnsignedClaimHandle, UnsignedCreateInbox,
        },
        unverified::{
            UnverifiedAction, UnverifiedChangeRecoveryAddress, UnverifiedClaimHandle,
            UnverifiedCreateInbox, UnverifiedIdentityUpdate, UnverifiedSignature,
        },
    },
    key_package::{KeyPackageOptions, build_key_package, create_credential},
    scw_verifier::{BlockStamp, ChainBlocks, VerifierError},
    utils::passkey::PasskeyUser,
};
use xmtp_mls_common::group_keys::{
    GroupKeys, GroupRoot, seal_commit_log_entry, sign_group_message,
};
use xmtp_proto::xmtp::{
    backend::v1::{
        ClientEnvelope, GroupMessage, KeyPackage, WelcomeMessage,
        client_envelope::Payload,
        welcome_message::{V1, Version, WelcomePointer},
    },
    identity::associations::{IdentifierKind, IdentityUpdate},
    mls::message_contents::PlaintextCommitLogEntry,
};

use std::sync::{Arc, Mutex};

pub const INSTALLATION_ID: [u8; 32] = [0x22; 32];

/// A controlled chain producing one block per second, so block `n` has
/// timestamp `n`. Every chain id and every clone share one head; a head of
/// `None` is an outage that fails every call with a retryable error.
#[derive(Debug, Clone)]
pub struct TestChain(Arc<Mutex<Option<u64>>>);

impl TestChain {
    pub fn at(head: u64) -> Self {
        Self(Arc::new(Mutex::new(Some(head))))
    }

    pub fn set(&self, head: Option<u64>) {
        *self.0.lock().expect("test chain lock") = head;
    }

    fn head_number(&self) -> Result<u64, VerifierError> {
        self.0
            .lock()
            .expect("test chain lock")
            .ok_or_else(|| VerifierError::Io(std::io::Error::other("test chain is down")))
    }
}

#[xmtp_common::async_trait]
impl ChainBlocks for TestChain {
    async fn head(&self, _: &str) -> Result<BlockStamp, VerifierError> {
        let number = self.head_number()?;
        Ok(BlockStamp {
            number,
            timestamp: number,
        })
    }

    async fn timestamp(&self, _: &str, number: u64) -> Result<u64, VerifierError> {
        if number > self.head_number()? {
            return Err(VerifierError::MissingBlock(number));
        }
        Ok(number)
    }
}

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
    let config = MlsGroupCreateConfig::test_default(xmtp_cryptography::configuration::CIPHERSUITE);
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
        // The fixture group holds no group root, and only the commit leaves.
        #[allow(clippy::disallowed_methods)]
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
            signature: vec![],
        })),
    }
}

/// The keys of the keyed group whose root is 32 copies of `seed`, so a
/// fixture names the same group on every run.
pub fn group_keys(seed: u8) -> GroupKeys {
    GroupKeys::derive(&GroupRoot::from_bytes(&[seed; 32]).expect("a fixture root is 32 bytes"))
}

/// A group message of the keyed group of `keys`, signed under its group
/// signing key as a client publishes it. Change a signed field afterwards to
/// build a message whose signature no longer verifies.
pub fn keyed_group_message_envelope(
    keys: &GroupKeys,
    kind: GroupMessageKind,
    trailing_bytes: impl AsRef<[u8]>,
) -> ClientEnvelope {
    let mut envelope = group_message_envelope(keys.group_id(), kind, trailing_bytes);
    let Some(Payload::GroupMessage(message)) = envelope.payload.as_mut() else {
        unreachable!("the fixture is a group message")
    };
    sign_group_message(keys, message).expect("fixture signing passes the self-check");
    envelope
}

/// A sealed commit-log entry of the keyed group of `keys` at
/// `commit_sequence_id`, as a client publishes it.
pub fn sealed_commit_log_envelope(keys: &GroupKeys, commit_sequence_id: u64) -> ClientEnvelope {
    let entry = PlaintextCommitLogEntry {
        group_id: keys.group_id().to_vec(),
        commit_sequence_id,
        last_epoch_authenticator: vec![0x60, 0x61],
        commit_result: 1,
        applied_epoch_number: 8,
        applied_epoch_authenticator: vec![0x62, 0x63],
        ..Default::default()
    };
    ClientEnvelope {
        payload: Some(Payload::SealedCommitLogEntry(
            seal_commit_log_entry(keys, &entry.encode_to_vec())
                .expect("fixture sealing passes the self-check"),
        )),
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
            signature: vec![],
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

/// The `last_resort_key_package` application component of the MLS
/// extensions draft, which marks a key package and carries empty data.
pub const LAST_RESORT_COMPONENT: u16 = 0x0004;

/// Build a key package whose `app_data_dictionary` extension holds
/// `components`, with capabilities that advertise the dictionary.
pub fn app_data_key_package_envelope(components: &[(u16, Vec<u8>)]) -> KeyPackageFixture {
    let inbox_id = "app-data-key-package".to_string();
    let provider = OpenMlsRustCrypto::default();
    let installation_keys = XmtpInstallationCredential::new();
    let mut dictionary = AppDataDictionary::new();
    for (component, data) in components {
        let _ = dictionary.insert(*component, data.clone());
    }
    let ciphersuite = xmtp_cryptography::configuration::CIPHERSUITE;
    let bundle = OpenMlsKeyPackage::builder()
        .leaf_node_capabilities(Capabilities::new(
            None,
            Some(&[ciphersuite]),
            Some(&[ExtensionType::AppDataDictionary]),
            None,
            None,
        ))
        .key_package_extensions(
            Extensions::from_vec(vec![Extension::AppDataDictionary(
                AppDataDictionaryExtension::new(dictionary),
            )])
            .expect("one AppData dictionary is a valid extension list"),
        )
        .build(
            ciphersuite,
            &provider,
            &installation_keys,
            CredentialWithKey {
                credential: create_credential(&inbox_id),
                signature_key: installation_keys.public_slice().into(),
            },
        )
        .expect("AppData key-package fixture builds");
    let tls_bytes = bundle
        .key_package()
        .tls_serialize_detached()
        .expect("AppData key-package fixture serializes");
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
            xmtp_cryptography::configuration::CIPHERSUITE,
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
const SCW_ACCOUNT: &str = "0x1111111111111111111111111111111111111111";

fn scw_signature(bytes: u8, block_number: u64) -> UnverifiedSignature {
    UnverifiedSignature::new_smart_contract_wallet(
        vec![bytes],
        AccountId::new_evm(1, SCW_ACCOUNT.to_string()),
        block_number,
    )
}

fn scw_inbox_id() -> String {
    Identifier::eth(SCW_ACCOUNT)
        .expect("fixture account is valid")
        .inbox_id(0)
        .expect("fixture account derives an inbox")
}

/// Create the fixture wallet's inbox with an ERC-6492 signature at block 1.
pub fn scw_create_inbox_update() -> IdentityUpdate {
    scw_create_inbox_update_at(1)
}

/// Create the fixture wallet's inbox with an ERC-6492 signature at `block_number`.
pub fn scw_create_inbox_update_at(block_number: u64) -> IdentityUpdate {
    UnverifiedIdentityUpdate::new(
        scw_inbox_id(),
        1,
        vec![UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
            UnsignedCreateInbox {
                account_identifier: Identifier::eth(SCW_ACCOUNT).expect("fixture account is valid"),
                nonce: 0,
            },
            scw_signature(0x55, block_number),
        ))],
    )
    .into()
}

/// Set the fixture inbox's recovery identifier to the wallet itself, its only
/// member, signed by the wallet at `block_number`. It applies after
/// [`scw_create_inbox_update_at`].
pub fn scw_change_recovery_update_at(block_number: u64) -> IdentityUpdate {
    UnverifiedIdentityUpdate::new(
        scw_inbox_id(),
        2,
        vec![UnverifiedAction::ChangeRecoveryAddress(
            UnverifiedChangeRecoveryAddress::new(
                UnsignedChangeRecoveryAddress {
                    new_recovery_identifier: Identifier::eth(SCW_ACCOUNT)
                        .expect("fixture account is valid"),
                },
                scw_signature(0x56, block_number),
            ),
        )],
    )
    .into()
}

/// Claim `handle` for the fixture inbox, signed by the wallet at
/// `block_number`. The signature's bytes are `signature`, and the update's
/// time is `signature` seconds. A signature text holds whole seconds, so two
/// claims with two bytes have two texts and two replay keys. It applies after
/// [`scw_create_inbox_update_at`].
pub fn scw_claim_handle_update_at(
    handle: &str,
    signature: u8,
    block_number: u64,
) -> IdentityUpdate {
    UnverifiedIdentityUpdate::new(
        scw_inbox_id(),
        u64::from(signature) * xmtp_common::NS_IN_SEC.unsigned_abs(),
        vec![UnverifiedAction::ClaimHandle(UnverifiedClaimHandle::new(
            UnsignedClaimHandle {
                handle: handle.to_string(),
            },
            scw_signature(signature, block_number),
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

/// A signed inbox creation, and a signed update that hands its recovery role
/// to an uppercase Ethereum identifier.
pub async fn mixed_case_recovery_change() -> (IdentityUpdate, IdentityUpdate) {
    let original_recovery = generate_local_wallet();
    let next_recovery = generate_local_wallet();
    let original_identifier = original_recovery.identifier();
    let inbox_id = original_recovery.get_inbox_id(0);

    let mut create = SignatureRequestBuilder::new(&inbox_id)
        .create_inbox(original_identifier.clone(), 0)
        .build();
    add_wallet_signature(&mut create, &original_recovery).await;
    let create = create
        .build_identity_update()
        .expect("signed create update is complete")
        .into();

    let mixed_case = next_recovery.identifier().to_string().to_ascii_uppercase();
    let mixed_case = Identifier::from_proto(&mixed_case, IdentifierKind::Ethereum, None)
        .expect("the builder takes an Ethereum identifier as given");
    let mut change = SignatureRequestBuilder::new(&inbox_id)
        .change_recovery_address(original_identifier.into(), mixed_case)
        .build();
    add_wallet_signature(&mut change, &original_recovery).await;
    let change = change
        .build_identity_update()
        .expect("signed recovery update is complete")
        .into();

    (create, change)
}

/// The bytes of a `ClientEnvelope` whose payload is field 5, the retired
/// `CommitLogEntry`, as a client from before the cutover encoded it: a
/// serialized `PlaintextCommitLogEntry` for `group_id`, with no signature.
/// Field 5 is reserved, so these bytes decode to an envelope with no payload.
pub fn retired_commit_log_envelope_bytes(group_id: impl AsRef<[u8]>) -> Vec<u8> {
    let entry = PlaintextCommitLogEntry {
        group_id: group_id.as_ref().to_vec(),
        commit_sequence_id: 7,
        last_epoch_authenticator: vec![0x60, 0x61],
        commit_result: 1,
        applied_epoch_number: 8,
        applied_epoch_authenticator: vec![0x62, 0x63],
        ..Default::default()
    }
    .encode_to_vec();
    let mut retired = Vec::new();
    prost::encoding::bytes::encode(1, &entry, &mut retired);
    let mut envelope = Vec::new();
    prost::encoding::bytes::encode(5, &retired, &mut envelope);
    envelope
}

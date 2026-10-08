use super::*;
use crate::test_utils::*;
use openmls::prelude::{
    Ciphersuite, CredentialWithKey, KeyPackage as OpenMlsKeyPackage, SignatureScheme,
    tls_codec::Serialize,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use xmtp_common::RetryableError;
use xmtp_id::{
    associations::{
        AssociationError, SignatureError, test_utils::MockSmartContractSignatureVerifier,
    },
    key_package::{KeyPackageOptions, create_credential},
    scw_verifier::{BlockStamp, ChainBlocks, MultiSmartContractSignatureVerifier, VerifierError},
};
use xmtp_proto::xmtp::backend::v1::{
    ClientEnvelope, KeyPackage, WelcomeMessage, client_envelope::Payload,
};

fn expected_topic(kind: TopicKind, identifier: impl AsRef<[u8]>) -> Vec<u8> {
    kind.create(identifier).cloned_vec()
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-001, API-230
fn group_message_matrix_preserves_routing_bytes_and_flags() {
    for (kind, expected_flag) in [
        (GroupMessageKind::Application, false),
        (GroupMessageKind::Proposal, true),
        (GroupMessageKind::Commit, true),
    ] {
        let envelope = group_message_envelope(GROUP_ID, kind, [0xaa, 0xbb]);
        let expected_data = match envelope.payload.as_ref() {
            Some(Payload::GroupMessage(group)) => group.data.clone(),
            _ => unreachable!("fixture is a group message"),
        };
        let parsed = parse_envelope(envelope)?;
        assert_eq!(
            parsed.topic.cloned_vec(),
            expected_topic(TopicKind::GroupMessagesV1, GROUP_ID)
        );
        assert_eq!(parsed.is_commit_or_proposal, expected_flag);
        let Some(Payload::GroupMessage(group)) = parsed.envelope.payload else {
            unreachable!("parsed envelope remains a group message")
        };
        assert_eq!(group.data, expected_data);
        assert!(group.data.ends_with(&[0xaa, 0xbb]));
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-002, API-230
fn group_message_matrix_rejects_malformed_data_and_wrong_id_lengths() {
    for group_id in [vec![0x11; 15], vec![0x11; 17]] {
        let error = parse_envelope(group_message_envelope(
            group_id,
            GroupMessageKind::Application,
            [],
        ))
        .err()
        .expect("group identifiers must be 16 bytes");
        assert_eq!(error.reason(), Reason::MalformedPayload);
    }

    let mut malformed = group_message_envelope(GROUP_ID, GroupMessageKind::Application, []);
    let Some(Payload::GroupMessage(group)) = malformed.payload.as_mut() else {
        unreachable!("fixture is a group message")
    };
    group.data = vec![0xff];
    let error = parse_envelope(malformed)
        .err()
        .expect("malformed MLS bytes must fail");
    assert_eq!(error.reason(), Reason::MalformedPayload);

    let error = parse_envelope(non_protocol_group_message_envelope())
        .err()
        .expect("non-protocol MLS bodies must fail");
    assert!(matches!(error, ValidationError::Protocol(_)));
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-001, API-230
fn welcome_matrix_accepts_both_forms_without_decryption() {
    for envelope in [
        inline_welcome_envelope(INSTALLATION_ID),
        welcome_pointer_envelope(INSTALLATION_ID),
    ] {
        let expected = envelope.clone();
        let parsed = parse_envelope(envelope)?;
        assert_eq!(
            parsed.topic.cloned_vec(),
            expected_topic(TopicKind::WelcomeMessagesV1, INSTALLATION_ID)
        );
        assert!(!parsed.is_commit_or_proposal);
        assert_eq!(parsed.envelope, expected);
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-002, API-230
fn welcome_matrix_rejects_missing_form_and_wrong_destination_lengths() {
    for installation_id in [vec![0x22; 31], vec![0x22; 33]] {
        let error = parse_envelope(inline_welcome_envelope(installation_id))
            .err()
            .expect("welcome destinations must be 32 bytes");
        assert_eq!(error.reason(), Reason::MalformedPayload);
    }

    let error = parse_envelope(ClientEnvelope {
        payload: Some(Payload::WelcomeMessage(WelcomeMessage { version: None })),
    })
    .err()
    .expect("welcome form must be present");
    assert!(matches!(error, ValidationError::MissingWelcomeVersion));

    let error = parse_envelope(ClientEnvelope { payload: None })
        .err()
        .expect("envelope payload must be present");
    assert!(matches!(error, ValidationError::MissingPayload));
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-001, API-230
async fn commit_log_admission_preserves_unverified_bytes_and_signature() {
    let mut envelope = commit_log_envelope(GROUP_ID);
    let Some(Payload::CommitLogEntry(entry)) = envelope.payload.as_mut() else {
        unreachable!("fixture is a commit-log entry")
    };
    let signature = entry
        .signature
        .as_mut()
        .expect("fixture has a commit-log signature");
    signature.bytes = vec![0x70];
    signature.public_key = vec![0x71];
    let expected = envelope.clone();
    let parsed = parse_envelope(envelope)?;
    assert_eq!(
        parsed.topic.cloned_vec(),
        expected_topic(TopicKind::CommitLogEntriesV1, GROUP_ID)
    );
    assert!(!parsed.is_commit_or_proposal);
    assert_eq!(parsed.envelope, expected);
    validate_envelope(
        &parsed,
        &[],
        MockSmartContractSignatureVerifier::new(false),
        &TestChain::at(1),
    )
    .await?;
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-002, API-230
fn commit_log_matrix_rejects_malformed_data_and_wrong_id_lengths() {
    for group_id in [vec![0x11; 15], vec![0x11; 17]] {
        let error = parse_envelope(commit_log_envelope(group_id))
            .err()
            .expect("commit-log group identifiers must be 16 bytes");
        assert_eq!(error.reason(), Reason::MalformedPayload);
    }

    let mut malformed = commit_log_envelope(GROUP_ID);
    let Some(Payload::CommitLogEntry(entry)) = malformed.payload.as_mut() else {
        unreachable!("fixture is a commit-log entry")
    };
    entry.serialized_commit_log_entry = vec![0xff];
    let error = parse_envelope(malformed)
        .err()
        .expect("malformed plaintext entry must fail");
    assert_eq!(error.reason(), Reason::MalformedPayload);
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: API-235, JOIN-004
async fn key_package_admission_accepts_the_existing_credential_shape() {
    let twenty_years = 20 * 365 * 24 * 60 * 60;
    for fixture in [
        key_package_envelope("", KeyPackageOptions::default()),
        key_package_envelope("not-a-hex-inbox", KeyPackageOptions::default()),
        minimal_key_package_envelope(
            "minimal-capabilities",
            xmtp_configuration::CIPHERSUITE,
            None,
        ),
        minimal_key_package_envelope(
            "long-lifetime",
            xmtp_configuration::CIPHERSUITE,
            Some(openmls::key_packages::Lifetime::new(twenty_years)),
        ),
        minimal_key_package_envelope(
            "alternate-ed25519-ciphersuite",
            Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            None,
        ),
    ] {
        let parsed = parse_envelope(fixture.envelope)?;
        assert_eq!(
            parsed.topic.cloned_vec(),
            expected_topic(TopicKind::KeyPackagesV1, &fixture.installation_id)
        );
        assert!(
            validate_envelope(
                &parsed,
                &[],
                MockSmartContractSignatureVerifier::new(false),
                &TestChain::at(1),
            )
            .await?
            .is_none()
        );
        assert_eq!(
            verify_key_package(&fixture.tls_bytes)?.credential.inbox_id,
            fixture.inbox_id
        );
    }
}

fn p256_key_package_envelope() -> (ClientEnvelope, Vec<u8>) {
    let ciphersuite = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256;
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::new(SignatureScheme::ECDSA_SECP256R1_SHA256)
        .expect("P-256 fixture key generation succeeds");
    let bundle = OpenMlsKeyPackage::builder()
        .build(
            ciphersuite,
            &provider,
            &signer,
            CredentialWithKey {
                credential: create_credential("p256-key-package"),
                signature_key: signer.to_public_vec().into(),
            },
        )
        .expect("P-256 fixture key package builds");
    let bytes = bundle
        .key_package()
        .tls_serialize_detached()
        .expect("P-256 fixture key package serializes");
    (
        ClientEnvelope {
            payload: Some(Payload::KeyPackage(KeyPackage {
                key_package_tls_serialized: bytes.clone(),
            })),
        },
        bytes,
    )
}

#[xmtp_common::test(unwrap_try = true)]
fn key_package_topic_length_is_separate_from_ciphersuite_validation() {
    let (envelope, bytes) = p256_key_package_envelope();
    assert!(verify_key_package(&bytes).is_ok());
    let error = parse_envelope(envelope)
        .err()
        .expect("P-256 signature keys do not satisfy the 32-byte topic contract");
    assert!(matches!(&error, ValidationError::Conversion(_)));
    assert_eq!(error.reason(), Reason::MalformedPayload);
}

// verifies: JOIN-007, JOIN-008
#[xmtp_common::test(unwrap_try = true)]
async fn key_package_parse_precedes_existing_cryptographic_validation() {
    for fixture in [
        mismatched_key_package_envelope(),
        expired_key_package_envelope(),
    ] {
        let parsed = parse_envelope(fixture.envelope)?;
        assert_eq!(
            parsed.topic.cloned_vec(),
            expected_topic(TopicKind::KeyPackagesV1, &fixture.installation_id)
        );
        let error = validate_envelope(
            &parsed,
            &[],
            MockSmartContractSignatureVerifier::new(false),
            &TestChain::at(1),
        )
        .await
        .err()
        .expect("semantic key-package validation must still run");
        assert!(matches!(&error, ValidationError::KeyPackage(_)));
        assert_eq!(error.reason(), Reason::InvalidKeyPackage);
    }

    let fixture = key_package_envelope("exact-tls", KeyPackageOptions::default());
    let mut trailing = fixture.envelope;
    let Some(Payload::KeyPackage(package)) = trailing.payload.as_mut() else {
        unreachable!("fixture is a key package")
    };
    package.key_package_tls_serialized.push(0);
    assert!(matches!(
        parse_envelope(trailing),
        Err(ValidationError::Tls(_))
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn identity_admission_folds_real_history_and_a_passkey_update() {
    let fixture = identity_history_with_passkey().await;
    let parsed = parse_envelope(identity_envelope(fixture.update.clone()))?;
    assert_eq!(
        parsed.topic.cloned_vec(),
        expected_topic(
            TopicKind::IdentityUpdatesV1,
            hex::decode(&fixture.inbox_id)?
        )
    );
    let result = validate_envelope(
        &parsed,
        &fixture.history,
        MockSmartContractSignatureVerifier::new(false),
        &TestChain::at(1),
    )
    .await?
    .expect("identity admission returns state and diff");
    assert!(result.state.get(&fixture.added_identifier).is_some());
    assert_eq!(result.diff.new_members, vec![fixture.added_identifier]);
    assert!(result.diff.removed_members.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: IDENT-050
async fn identity_admission_preserves_typed_replay_failure() {
    let fixture = identity_history_with_passkey().await;
    let mut history = fixture.history;
    history.push(fixture.update.clone());
    let error = validate_identity_updates(
        history,
        vec![fixture.update],
        MockSmartContractSignatureVerifier::new(false),
    )
    .await
    .err()
    .expect("reused passkey and recovery signatures must fail");
    assert!(matches!(
        &error,
        ValidationError::Association(AssociationError::Replay)
    ));
    assert_eq!(error.reason(), Reason::InvalidIdentityUpdate);
    assert!(!error.is_retryable());
}

// verifies: IDENT-061
#[xmtp_common::test(unwrap_try = true)]
async fn identity_admission_preserves_retryable_scw_failure() {
    let verifier = MultiSmartContractSignatureVerifier::new(Default::default())?;
    let error = validate_identity_updates(vec![], vec![scw_create_inbox_update()], verifier)
        .await
        .err()
        .expect("missing chain verifier must remain retryable");
    assert!(matches!(
        &error,
        ValidationError::Signature(SignatureError::VerifierError(VerifierError::NoVerifier(_)))
    ));
    assert_eq!(error.reason(), Reason::InvalidSignature);
    assert!(error.is_retryable());
}

// verifies: IDENT-030
#[xmtp_common::test(unwrap_try = true)]
async fn identity_signature_failure_precedes_state_application() {
    let error = validate_identity_updates(
        vec![],
        vec![malformed_signature_create_inbox_update()],
        MockSmartContractSignatureVerifier::new(false),
    )
    .await
    .err()
    .expect("malformed signature must fail before inbox creation");
    assert!(matches!(&error, ValidationError::Signature(_)));
    assert_eq!(error.reason(), Reason::InvalidSignature);
    assert!(!error.is_retryable());
}

/// An uppercase recovery identifier is rejected when it is admitted and when a
/// stored log carrying it is replayed, so no signer can be locked out of recovery.
// verifies: IDENT-013
#[xmtp_common::test(unwrap_try = true)]
async fn identity_invalid_mutations_mixed_case_recovery_is_rejected() {
    let (create, change) = mixed_case_recovery_change().await;
    for (history, new) in [
        (vec![create.clone()], vec![change.clone()]),
        (vec![create, change], vec![]),
    ] {
        let error =
            validate_identity_updates(history, new, MockSmartContractSignatureVerifier::new(false))
                .await
                .err()
                .expect("a mixed-case recovery identifier must be rejected");
        assert!(matches!(
            error,
            ValidationError::Conversion(ConversionError::InvalidValue {
                item: "ethereum identifier",
                ..
            })
        ));
        assert!(!error.is_retryable());
    }
}

#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-002
fn identity_topics_require_a_32_byte_hex_inbox() {
    let fixture = xmtp_proto::xmtp::identity::associations::IdentityUpdate {
        inbox_id: "11".repeat(31),
        ..Default::default()
    };
    let error = parse_envelope(identity_envelope(fixture))
        .err()
        .expect("identity topic identifiers must be 32 bytes");
    assert_eq!(error.reason(), Reason::MalformedPayload);

    let fixture = xmtp_proto::xmtp::identity::associations::IdentityUpdate {
        inbox_id: "not-hex".into(),
        ..Default::default()
    };
    assert!(matches!(
        parse_envelope(identity_envelope(fixture)),
        Err(ValidationError::Inbox(_))
    ));
}

/// Reports a head stamped before its own earlier blocks, as separate reads
/// straddling a reorg can.
struct SkewedChain;

#[xmtp_common::async_trait]
impl ChainBlocks for SkewedChain {
    async fn head(&self, _: &str) -> Result<BlockStamp, VerifierError> {
        Ok(BlockStamp {
            number: 10,
            timestamp: 100,
        })
    }

    async fn timestamp(&self, _: &str, _: u64) -> Result<u64, VerifierError> {
        Ok(200)
    }
}

/// The age check must fail closed: a block whose timestamp exceeds the head's
/// has no provable age, and treating it as fresh would let a removed signer
/// replay an old signature through an inconsistent chain read.
#[xmtp_common::test]
// verifies: IDENT-062
async fn freshness_rejects_a_block_stamped_after_the_head() {
    assert!(matches!(
        check_freshness(&scw_create_inbox_update_at(5), &SkewedChain).await,
        Err(ValidationError::StaleBlock(5))
    ));
}

/// Counts reads and, like a chain the deployment does not route, answers
/// each with a retryable missing route.
#[derive(Default)]
struct RoutelessChain(std::sync::atomic::AtomicUsize);

#[xmtp_common::async_trait]
impl ChainBlocks for RoutelessChain {
    async fn head(&self, chain_id: &str) -> Result<BlockStamp, VerifierError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(VerifierError::NoVerifier(chain_id.into()))
    }

    async fn timestamp(&self, chain_id: &str, _: u64) -> Result<u64, VerifierError> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(VerifierError::NoVerifier(chain_id.into()))
    }
}

/// A malformed account id anywhere in an update, in its chain id or its
/// address, rejects it permanently before any chain access, even behind a
/// well-formed signature and on a chain with no route, so an invalid update
/// cannot buy RPC work or wait as retryable for ever. A well-formed one on
/// that chain reaches it and stays retryable.
#[xmtp_common::test]
// verifies: IDENT-060, IDENT-061
async fn freshness_rejects_any_malformed_account_before_chain_access() {
    let chain = RoutelessChain::default();
    for malformed in [
        "eip155:01:0x1111111111111111111111111111111111111111",
        "eip155:1:bad",
        "eip155:1:1111111111111111111111111111111111111111",
        "eip155:1:0x111111111111111111111111111111111111111g",
        "eip155:1:0x11111111111111111111111111111111111111111",
    ] {
        let update = with_second_signature(|scw| scw.account_id = malformed.into());
        let error = check_freshness(&update, &chain).await.expect_err(malformed);
        assert!(!error.is_retryable(), "{malformed}: {error}");
        assert_eq!(error.reason(), Reason::InvalidSignature, "{malformed}");
    }
    assert_eq!(chain.0.load(std::sync::atomic::Ordering::Relaxed), 0);

    let error = check_freshness(&scw_create_inbox_update_at(1), &chain)
        .await
        .expect_err("a chain without a route cannot judge freshness");
    assert!(error.is_retryable());
    assert_eq!(chain.0.into_inner(), 1);
}

/// Append a copy of the fixture inbox's ERC-6492 signature, edited by `edit`.
fn with_second_signature(
    edit: impl FnOnce(&mut xmtp_proto::xmtp::identity::associations::SmartContractWalletSignature),
) -> IdentityUpdate {
    use xmtp_proto::xmtp::identity::associations::{identity_action, signature};
    let mut update = scw_create_inbox_update_at(1);
    let mut second = update.actions[0].clone();
    let Some(identity_action::Kind::CreateInbox(create)) = &mut second.kind else {
        unreachable!("fixture creates an inbox")
    };
    let Some(signature::Signature::Erc6492(scw)) = create
        .initial_identifier_signature
        .as_mut()
        .and_then(|value| value.signature.as_mut())
    else {
        unreachable!("fixture signs with a smart-contract wallet")
    };
    edit(scw);
    update.actions.push(second);
    update
}

/// Counts the reads it forwards to a [`TestChain`].
#[derive(Default)]
struct CountingChain {
    heads: std::sync::atomic::AtomicUsize,
    timestamps: std::sync::atomic::AtomicUsize,
}

#[xmtp_common::async_trait]
impl ChainBlocks for CountingChain {
    async fn head(&self, chain_id: &str) -> Result<BlockStamp, VerifierError> {
        self.heads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        TestChain::at(10).head(chain_id).await
    }

    async fn timestamp(&self, chain_id: &str, number: u64) -> Result<u64, VerifierError> {
        self.timestamps
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        TestChain::at(10).timestamp(chain_id, number).await
    }
}

/// An update may carry up to the configured signature limit, so admission
/// reads each chain's head once and each distinct block's timestamp once
/// rather than once per signature.
#[xmtp_common::test(unwrap_try = true)]
// verifies: IDENT-062
async fn freshness_reads_each_head_and_block_once() {
    let chain = CountingChain::default();
    check_freshness(&with_second_signature(|_| {}), &chain).await?;
    check_freshness(&with_second_signature(|scw| scw.block_number = 2), &chain).await?;
    assert_eq!(chain.heads.into_inner(), 2);
    assert_eq!(chain.timestamps.into_inner(), 3);
}

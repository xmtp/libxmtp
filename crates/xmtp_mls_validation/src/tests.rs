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
    scw_verifier::{MultiSmartContractSignatureVerifier, VerifierError},
};
use xmtp_proto::xmtp::backend::v1::{
    ClientEnvelope, KeyPackage, WelcomeMessage, client_envelope::Payload,
};

fn expected_topic(kind: TopicKind, identifier: impl AsRef<[u8]>) -> Vec<u8> {
    kind.create(identifier).cloned_vec()
}

#[xmtp_common::test(unwrap_try = true)]
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
    validate_envelope(&parsed, &[], MockSmartContractSignatureVerifier::new(false)).await?;
}

#[xmtp_common::test(unwrap_try = true)]
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
async fn key_package_admission_accepts_the_existing_credential_shape() {
    let twenty_years = 20 * 365 * 24 * 60 * 60;
    for fixture in [
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
            validate_envelope(&parsed, &[], MockSmartContractSignatureVerifier::new(false))
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
        let error = validate_envelope(&parsed, &[], MockSmartContractSignatureVerifier::new(false))
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
    )
    .await?
    .expect("identity admission returns state and diff");
    assert!(result.state.get(&fixture.added_identifier).is_some());
    assert_eq!(result.diff.new_members, vec![fixture.added_identifier]);
    assert!(result.diff.removed_members.is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
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

#[xmtp_common::test(unwrap_try = true)]
async fn identity_admission_preserves_raw_recovery_identifier_behavior() {
    let fixture = identity_history_with_raw_recovery().await;
    let state = validate_identity_updates(
        fixture.history.clone(),
        vec![],
        MockSmartContractSignatureVerifier::new(false),
    )
    .await?;
    assert_eq!(
        state.state.recovery_identifier().to_string(),
        fixture.raw_recovery_identifier
    );

    let error = validate_identity_updates(
        fixture.history,
        vec![fixture.rejected_update],
        MockSmartContractSignatureVerifier::new(false),
    )
    .await
    .err()
    .expect("canonical signer must not match the retained raw recovery value");
    assert!(matches!(
        error,
        ValidationError::Association(AssociationError::MissingExistingMember)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
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

use crate::test_support as support;

use crate::api;
use prost::Message;
use support::TestServer;
use tonic::Code;
use xmtp_mls_validation::test_utils::{
    identity_envelope, identity_history_with_passkey, scw_create_inbox_update,
};
use xmtp_proto::xmtp::identity::associations::IdentifierKind;

#[xmtp_common::test(unwrap_try = true)]
async fn installation_members_are_excluded_from_identifier_lookup_projection() {
    use xmtp_id::associations::{
        MemberIdentifier,
        builder::SignatureRequestBuilder,
        test_utils::{WalletTestExt, add_installation_key_signature, add_wallet_signature},
    };
    let server = TestServer::new(|_| {}).await?;
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let installation = xmtp_cryptography::basic_credential::XmtpInstallationCredential::new();
    let identifier = wallet.identifier();
    let inbox = wallet.get_inbox_id(0);
    let installation_key = installation.public_bytes().to_vec();
    let mut request = SignatureRequestBuilder::new(&inbox)
        .create_inbox(identifier.clone(), 0)
        .add_association(
            MemberIdentifier::installation(installation_key.clone()),
            identifier.clone().into(),
        )
        .build();
    add_wallet_signature(&mut request, &wallet).await;
    add_installation_key_signature(&mut request, &installation).await;
    server
        .publish(vec![identity_envelope(
            request.build_identity_update()?.into(),
        )])
        .await?;
    let rows: Vec<(String, i16)> = sqlx::query_as(
        "SELECT identifier, identifier_kind FROM identifier_association ORDER BY identifier",
    )
    .fetch_all(&server.backend.store.primary)
    .await?;
    assert_eq!(rows, vec![(identifier.to_string().to_ascii_lowercase(), 1)]);
    let result = server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest {
            requests: vec![
                api::get_inbox_ids_request::Request {
                    identifier: hex::encode(installation_key),
                    identifier_kind: IdentifierKind::Passkey.into(),
                },
                api::get_inbox_ids_request::Request {
                    identifier: identifier.to_string(),
                    identifier_kind: IdentifierKind::Ethereum.into(),
                },
            ],
        })
        .await?
        .into_inner();
    assert_eq!(result.responses[0].inbox_id, None);
    assert_eq!(result.responses[1].inbox_id, Some(inbox));
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn verified_identity_projects_normalized_positional_lookups_and_duplicate_retries() {
    let server = TestServer::new(|config| config.limits.max_identity_entries = 2).await?;
    let fixture = identity_history_with_passkey().await;
    server
        .publish(fixture.history.into_iter().map(identity_envelope).collect())
        .await?;
    let update = identity_envelope(fixture.update);
    let stored = server.publish(vec![update.clone()]).await?;
    assert_eq!(server.publish(vec![update]).await?, stored);
    let mut backend = server.backend.clone();
    std::sync::Arc::make_mut(&mut backend.config)
        .limits
        .max_identity_entries = 1;
    backend.config.validate()?;
    let topic = stored[0].topic.clone().unwrap();
    let history = backend.store.history(&topic.topic).await?;
    assert_eq!(history.payloads.len(), 2);
    use crate::api::query_service_server::QueryService;
    let page = QueryService::query(
        &backend,
        tonic::Request::new(api::QueryRequest {
            queries: vec![support::query_topic(topic, 0)],
            limit: 100,
        }),
    )
    .await?
    .into_inner();
    assert_eq!(page.envelopes.len(), 2);
    let identifier = fixture.added_identifier.to_string().to_uppercase();
    let input = api::get_inbox_ids_request::Request {
        identifier: identifier.clone(),
        identifier_kind: IdentifierKind::Passkey.into(),
    };
    let absent = api::get_inbox_ids_request::Request {
        identifier: "abcd".into(),
        identifier_kind: IdentifierKind::Passkey.into(),
    };
    let output = server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest {
            requests: vec![input.clone(), absent, input],
        })
        .await?
        .into_inner();
    assert_eq!(output.responses.len(), 3);
    assert_eq!(output.responses[0], output.responses[2]);
    assert_eq!(output.responses[0].identifier, identifier);
    assert_eq!(output.responses[0].inbox_id, Some(fixture.inbox_id));
    assert_eq!(output.responses[1].inbox_id, None);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn scw_missing_routes_are_unavailable_and_bad_request_shapes_are_invalid() {
    let server = TestServer::new(|_| {}).await?;
    let input = api::verify_smart_contract_wallet_signatures_request::Signature {
        account_id: "eip155:1:0x1111111111111111111111111111111111111111".into(),
        hash: vec![0; 32],
        signature: vec![1],
        block_number: Some(1),
    };
    assert_eq!(
        server
            .identity()
            .verify_smart_contract_wallet_signatures(
                api::VerifySmartContractWalletSignaturesRequest {
                    signatures: vec![input.clone()]
                }
            )
            .await
            .unwrap_err()
            .code(),
        Code::Unavailable
    );
    let mut malformed = input;
    malformed.hash.pop();
    assert_eq!(
        server
            .identity()
            .verify_smart_contract_wallet_signatures(
                api::VerifySmartContractWalletSignaturesRequest {
                    signatures: vec![malformed]
                }
            )
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    assert_eq!(
        server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![identity_envelope(scw_create_inbox_update())]
            })
            .await
            .unwrap_err()
            .code(),
        Code::Unavailable
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn identity_cap_rejects_new_updates_but_preserves_readable_history() {
    let server = TestServer::new(|config| config.limits.max_identity_entries = 1).await?;
    let fixture = identity_history_with_passkey().await;
    let first = identity_envelope(fixture.history[0].clone());
    let original = server.publish(vec![first.clone()]).await?;
    assert_eq!(
        server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![identity_envelope(fixture.update)]
            })
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    assert_eq!(server.publish(vec![first]).await?, original);
    let history = server
        .query()
        .query(api::QueryRequest {
            queries: vec![support::query_topic(original[0].topic.clone().unwrap(), 0)],
            limit: 0,
        })
        .await?
        .into_inner();
    assert_eq!(history.envelopes.len(), 1);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn revoking_latest_association_exposes_older_active_inbox() {
    use xmtp_id::associations::{
        builder::SignatureRequestBuilder,
        test_utils::{WalletTestExt, add_wallet_signature},
    };
    let server = TestServer::new(|_| {}).await?;
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let identifier = wallet.identifier();
    let mut inboxes = Vec::new();
    for nonce in [0, 1] {
        let inbox = identifier.inbox_id(nonce)?;
        let mut request = SignatureRequestBuilder::new(&inbox)
            .create_inbox(identifier.clone(), nonce)
            .build();
        add_wallet_signature(&mut request, &wallet).await;
        server
            .publish(vec![identity_envelope(
                request.build_identity_update()?.into(),
            )])
            .await?;
        inboxes.push(inbox);
    }
    let lookup = api::GetInboxIdsRequest {
        requests: vec![api::get_inbox_ids_request::Request {
            identifier: identifier.to_string().to_uppercase(),
            identifier_kind: IdentifierKind::Ethereum.into(),
        }],
    };
    assert_eq!(
        server
            .identity()
            .get_inbox_ids(lookup.clone())
            .await?
            .into_inner()
            .responses[0]
            .inbox_id,
        Some(inboxes[1].clone())
    );
    let mut revoke = SignatureRequestBuilder::new(&inboxes[1])
        .revoke_association(identifier.clone().into(), identifier.into())
        .build();
    add_wallet_signature(&mut revoke, &wallet).await;
    server
        .publish(vec![identity_envelope(
            revoke.build_identity_update()?.into(),
        )])
        .await?;
    assert_eq!(
        server
            .identity()
            .get_inbox_ids(lookup)
            .await?
            .into_inner()
            .responses[0]
            .inbox_id,
        Some(inboxes[0].clone())
    );
    server.stop().await?;
}

struct CountingVerifier(std::sync::Arc<std::sync::atomic::AtomicUsize>);
#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for CountingVerifier {
    async fn is_valid_signature(
        &self,
        _: xmtp_id::associations::AccountId,
        _: [u8; 32],
        _: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(xmtp_id::scw_verifier::ValidationResponse {
            is_valid: true,
            block_number,
            error: None,
        })
    }
}
struct VerdictVerifier;

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for VerdictVerifier {
    async fn is_valid_signature(
        &self,
        _: xmtp_id::associations::AccountId,
        hash: [u8; 32],
        _: alloy_primitives::Bytes,
        block_number: Option<u64>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        Ok(xmtp_id::scw_verifier::ValidationResponse {
            is_valid: hash[0] == 1,
            block_number: Some(block_number.unwrap_or(42)),
            error: None,
        })
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn lookup_limit_accepts_exact_count_and_rejects_one_past() {
    let server = TestServer::new(|config| config.limits.max_lookup_identifiers = 1).await?;
    let request = api::get_inbox_ids_request::Request {
        identifier: "abcd".into(),
        identifier_kind: IdentifierKind::Passkey.into(),
    };
    let response = server
        .identity()
        .get_inbox_ids(api::GetInboxIdsRequest {
            requests: vec![request.clone()],
        })
        .await?
        .into_inner();
    assert_eq!(response.responses.len(), 1);
    assert_eq!(response.responses[0].identifier, request.identifier);
    assert_eq!(response.responses[0].inbox_id, None);

    assert_eq!(
        server
            .identity()
            .get_inbox_ids(api::GetInboxIdsRequest {
                requests: vec![request.clone(), request],
            })
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn scw_signature_limit_accepts_exact_count_and_rejects_one_past() {
    let server = TestServer::with_verifier(
        |config| config.limits.max_scw_signatures = 1,
        VerdictVerifier,
    )
    .await?;
    let signature = api::verify_smart_contract_wallet_signatures_request::Signature {
        account_id: "eip155:1:0x1111111111111111111111111111111111111111".into(),
        hash: vec![1; 32],
        signature: vec![1],
        block_number: None,
    };
    let response = server
        .identity()
        .verify_smart_contract_wallet_signatures(api::VerifySmartContractWalletSignaturesRequest {
            signatures: vec![signature.clone()],
        })
        .await?
        .into_inner();
    assert_eq!(response.responses.len(), 1);
    assert!(response.responses[0].is_valid);
    assert_eq!(response.responses[0].block_number, Some(42));

    assert_eq!(
        server
            .identity()
            .verify_smart_contract_wallet_signatures(
                api::VerifySmartContractWalletSignaturesRequest {
                    signatures: vec![signature.clone(), signature],
                },
            )
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn identity_update_scw_signature_limit_is_checked_before_verification() {
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let server = TestServer::with_verifier(
        |config| config.limits.max_scw_signatures = 1,
        CountingVerifier(calls.clone()),
    )
    .await?;
    let update = scw_create_inbox_update();
    let stored = server
        .publish(vec![identity_envelope(update.clone())])
        .await?;
    assert_eq!(stored.len(), 1);
    let verified = calls.load(std::sync::atomic::Ordering::SeqCst);
    assert!(verified > 0);
    let mut excess = update.clone();
    let Some(xmtp_proto::xmtp::identity::associations::identity_action::Kind::CreateInbox(create)) =
        &mut excess.actions[0].kind
    else {
        panic!("expected create fixture");
    };
    let Some(xmtp_proto::xmtp::identity::associations::signature::Signature::Erc6492(signature)) =
        create
            .initial_identifier_signature
            .as_mut()
            .and_then(|signature| signature.signature.as_mut())
    else {
        panic!("expected SCW fixture");
    };
    signature.signature.push(0x99);
    signature.block_number += 1;
    excess.actions.extend(excess.actions.clone());
    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![identity_envelope(excess)],
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    assert_eq!(detail.index, Some(0));
    assert_eq!(detail.reason(), api::publish_error::Reason::TooLarge);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), verified);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, 1);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn scw_verdicts_preserve_input_order_and_resolved_blocks() {
    let server = TestServer::with_verifier(|_| {}, VerdictVerifier).await?;
    let signatures = [(1, None), (0, Some(12)), (1, Some(13))]
        .map(|(byte, block_number)| {
            api::verify_smart_contract_wallet_signatures_request::Signature {
                account_id: "eip155:1:0x1111111111111111111111111111111111111111".into(),
                hash: vec![byte; 32],
                signature: vec![1],
                block_number,
            }
        })
        .to_vec();
    let responses = server
        .identity()
        .verify_smart_contract_wallet_signatures(api::VerifySmartContractWalletSignaturesRequest {
            signatures,
        })
        .await?
        .into_inner()
        .responses;
    assert_eq!(
        responses
            .iter()
            .map(|response| (response.is_valid, response.block_number))
            .collect::<Vec<_>>(),
        vec![(true, Some(42)), (false, Some(12)), (true, Some(13))]
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn configured_chain_provider_failure_is_unavailable() {
    let unavailable = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = unavailable.local_addr()?;
    drop(unavailable);
    let server = TestServer::new(|config| {
        config
            .chains
            .insert("eip155:1".into(), format!("http://{address}"));
    })
    .await?;
    let signature = api::verify_smart_contract_wallet_signatures_request::Signature {
        account_id: "eip155:1:0x1111111111111111111111111111111111111111".into(),
        hash: vec![1; 32],
        signature: vec![1],
        block_number: Some(1),
    };
    let error = server
        .identity()
        .verify_smart_contract_wallet_signatures(api::VerifySmartContractWalletSignaturesRequest {
            signatures: vec![signature],
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::Unavailable);
    server.stop().await?;
}

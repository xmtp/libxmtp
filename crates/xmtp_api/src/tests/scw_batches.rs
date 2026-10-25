//! Smart contract wallet signatures verified in batched requests.

use super::wrapper;
use crate::{ApiClientWrapper, ApiError};
use prost::Message;
use std::sync::{Arc, Mutex};
use xmtp_api_backend::MockBackendClient;
use xmtp_common::RetryableError;
use xmtp_configuration::{
    LimitsConfiguration, MAX_SCW_SIGNATURE_BYTES, MAX_SCW_SIGNATURES_PER_REQUEST,
    ServerConfiguration,
};
use xmtp_id::{
    associations::AccountId,
    scw_verifier::{ScwInput, SmartContractSignatureVerifier},
};
use xmtp_proto::backend_v1 as wire;

/// `count` checked inputs, each with a signature of `signature_bytes` bytes
/// and the block number of its index.
fn inputs(count: usize, signature_bytes: usize) -> Vec<ScwInput> {
    (0..count)
        .map(|index| {
            ScwInput::checked(
                AccountId::new_evm(1, format!("0x{:040x}", index + 1)),
                [1; 32],
                vec![1; signature_bytes].into(),
                Some(index as u64),
            )
            .unwrap()
        })
        .collect()
}

/// A backend that answers every signature with a valid verdict at its own
/// block number, after it records the request's signature count and
/// encoded length.
fn echo_backend(requests: Arc<Mutex<Vec<(usize, usize)>>>) -> MockBackendClient {
    let mut mock = MockBackendClient::new();
    mock.expect_verify_smart_contract_wallet_signatures()
        .returning(move |request| {
            requests
                .lock()
                .unwrap()
                .push((request.signatures.len(), request.encoded_len()));
            Ok(wire::VerifySmartContractWalletSignaturesResponse {
                responses: request
                    .signatures
                    .into_iter()
                    .map(|signature| {
                        wire::verify_smart_contract_wallet_signatures_response::Response {
                            is_valid: true,
                            block_number: signature.block_number,
                            error: None,
                        }
                    })
                    .collect(),
            })
        });
    mock
}

fn with_limits(
    mut api: ApiClientWrapper<MockBackendClient>,
    limits: LimitsConfiguration,
) -> ApiClientWrapper<MockBackendClient> {
    api.set_configuration(Arc::new(ServerConfiguration {
        limits,
        ..ServerConfiguration::default()
    }));
    api
}

/// The block number of each verdict, which the echo backend sets to its
/// input's index.
async fn verified_blocks(
    verifier: impl SmartContractSignatureVerifier,
    inputs: &[ScwInput],
) -> Vec<Option<u64>> {
    verifier
        .verify_many(inputs)
        .await
        .unwrap()
        .into_iter()
        .map(|verdict| verdict.block_number)
        .collect()
}

fn in_order(count: usize) -> Vec<Option<u64>> {
    (0..count as u64).map(Some).collect()
}

/// A batch of signatures that fits one request is one RPC, also through the
/// `Arc<Box<dyn _>>` the client holds, and its verdicts keep input order.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn identity_scw_verifier_batches_signatures_in_one_rpc() {
    const SIGNATURES: usize = 8;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let verifier: Arc<Box<dyn SmartContractSignatureVerifier>> =
        Arc::new(Box::new(wrapper(echo_backend(requests.clone()))));
    assert_eq!(
        verified_blocks(&verifier, &inputs(SIGNATURES, 65)).await,
        in_order(SIGNATURES)
    );
    let counts: Vec<_> = requests.lock().unwrap().iter().map(|r| r.0).collect();
    assert_eq!(counts, vec![SIGNATURES]);
}

/// Requests end before a signature that would take them past the
/// published request bytes: five signatures of the largest size, two to a
/// request of at most 40,000 bytes, take three RPCs.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_splits_by_request_bytes() {
    const MAX_REQUEST_BYTES: usize = 40_000;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let api = with_limits(
        wrapper(echo_backend(requests.clone())),
        LimitsConfiguration {
            max_request_bytes: MAX_REQUEST_BYTES,
            ..LimitsConfiguration::default()
        },
    );
    assert_eq!(
        verified_blocks(&api, &inputs(5, MAX_SCW_SIGNATURE_BYTES)).await,
        in_order(5)
    );
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![2, 2, 1]
    );
    assert!(requests.iter().all(|r| r.1 <= MAX_REQUEST_BYTES));
}

/// A published `max_scw_signatures` above what one identity update can carry
/// is held at `MAX_SCW_SIGNATURES_PER_REQUEST`.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_size_is_clamped_to_protocol_maximum() {
    const SIGNATURES: usize = 300;
    let requests = Arc::new(Mutex::new(Vec::new()));
    let api = with_limits(
        wrapper(echo_backend(requests.clone())),
        LimitsConfiguration {
            max_scw_signatures: 1000,
            ..LimitsConfiguration::default()
        },
    );
    assert_eq!(
        verified_blocks(&api, &inputs(SIGNATURES, 65)).await,
        in_order(SIGNATURES)
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.0)
            .collect::<Vec<_>>(),
        vec![
            MAX_SCW_SIGNATURES_PER_REQUEST,
            SIGNATURES - MAX_SCW_SIGNATURES_PER_REQUEST
        ]
    );
}

/// A response with a verdict missing is a retryable `VerdictCount`, also
/// once the verifier wraps it, so the update behind it is retried, never
/// judged invalid.
// verifies: IDENT-061
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_with_wrong_verdict_count_is_retryable() {
    const SIGNATURES: usize = 8;
    let mut mock = MockBackendClient::new();
    mock.expect_verify_smart_contract_wallet_signatures()
        .times(2)
        .returning(|request| {
            Ok(wire::VerifySmartContractWalletSignaturesResponse {
                responses: request
                    .signatures
                    .iter()
                    .skip(1)
                    .map(
                        |_| wire::verify_smart_contract_wallet_signatures_response::Response {
                            is_valid: true,
                            block_number: Some(1),
                            error: None,
                        },
                    )
                    .collect(),
            })
        });
    let api = wrapper(mock);
    let inputs = inputs(SIGNATURES, 65);
    let error = api
        .verify_smart_contract_wallet_signatures(wire::VerifySmartContractWalletSignaturesRequest {
            signatures: inputs
                .iter()
                .map(
                    |input| wire::verify_smart_contract_wallet_signatures_request::Signature {
                        account_id: input.account_id().clone().into(),
                        block_number: input.block_number(),
                        hash: input.hash().to_vec(),
                        signature: input.signature().to_vec(),
                    },
                )
                .collect(),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ApiError::VerdictCount {
            expected: SIGNATURES,
            got: 7
        }
    ));
    assert!(error.is_retryable());
    let Err(error) = api.verify_many(&inputs).await else {
        panic!("a short answer must fail the batch");
    };
    assert!(error.is_retryable(), "{error}");
}

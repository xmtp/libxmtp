//! Smart contract wallet signatures verified in batched requests.

use super::{status, wrapper};
use crate::{ApiClientWrapper, ApiError};
use prost::Message;
use rstest::rstest;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering::SeqCst},
};
use xmtp_api_backend::MockBackendClient;
use xmtp_common::{Retry, RetryableError};
use xmtp_configuration::{
    LimitsConfiguration, MAX_SCW_SIGNATURE_BYTES, MAX_SCW_SIGNATURES_PER_REQUEST,
    ServerConfiguration,
};
use xmtp_id::{
    associations::AccountId,
    scw_verifier::{ScwInput, SmartContractSignatureVerifier},
};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

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

/// The verify request that carries `inputs`.
fn request(inputs: &[ScwInput]) -> wire::VerifySmartContractWalletSignaturesRequest {
    wire::VerifySmartContractWalletSignaturesRequest {
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
    }
}

/// The block number up to which a request is held.
const HOLD: u64 = 1_000;

/// A verify backend. It refuses a request of more than `max_signatures`
/// signatures with `refusal`, as a deployment with that limit does, and
/// answers each signature of any other request with a valid verdict at the
/// signature's block number. An answered request first yields once for each
/// block from its first signature's up to `HOLD`, so a later request
/// finishes first. The backend records each request's signature count and
/// encoded length, and the most answered requests in flight at once.
#[derive(Clone)]
struct VerifyBackend {
    max_signatures: usize,
    refusal: tonic::Code,
    /// The refusal carries the rate-limit detail.
    rate_limited: bool,
    requests: Arc<Mutex<Vec<(usize, usize)>>>,
    now: Arc<AtomicUsize>,
    most: Arc<AtomicUsize>,
}

impl VerifyBackend {
    fn refusing_over(max_signatures: usize, refusal: tonic::Code) -> Self {
        Self {
            max_signatures,
            refusal,
            rate_limited: false,
            requests: Arc::default(),
            now: Arc::default(),
            most: Arc::default(),
        }
    }

    fn new() -> Self {
        Self::refusing_over(usize::MAX, tonic::Code::InvalidArgument)
    }

    /// The signature count of each request, in the order they arrived.
    fn counts(&self) -> Vec<usize> {
        self.requests.lock().unwrap().iter().map(|r| r.0).collect()
    }

    /// A client of this backend with `limits` as its snapshot.
    fn client(&self, limits: LimitsConfiguration) -> ApiClientWrapper<Self> {
        let mut api = ApiClientWrapper::new(self.clone(), Retry::builder().retries(0).build());
        api.set_configuration(Arc::new(ServerConfiguration {
            limits,
            ..ServerConfiguration::default()
        }));
        api
    }
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for VerifyBackend {
    type Error = ApiClientError;

    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, ApiClientError> {
        self.requests
            .lock()
            .unwrap()
            .push((request.signatures.len(), request.encoded_len()));
        if request.signatures.len() > self.max_signatures {
            if self.rate_limited {
                let limited = xmtp_api_grpc::error::rate_limited_status(
                    "chain_verify",
                    xmtp_common::time::Duration::from_secs(300),
                );
                return Err(ApiClientError::client(
                    xmtp_api_grpc::error::GrpcError::Status(tonic::Status::with_details(
                        self.refusal,
                        "refused",
                        limited.details().to_vec().into(),
                    )),
                ));
            }
            return Err(status(self.refusal));
        }
        let now = self.now.fetch_add(1, SeqCst) + 1;
        self.most.fetch_max(now, SeqCst);
        let first = request.signatures[0].block_number.unwrap_or_default();
        for _ in first..HOLD {
            xmtp_common::task::yield_now().await;
        }
        self.now.fetch_sub(1, SeqCst);
        Ok(wire::VerifySmartContractWalletSignaturesResponse {
            responses: request
                .signatures
                .into_iter()
                .map(
                    |signature| wire::verify_smart_contract_wallet_signatures_response::Response {
                        is_valid: true,
                        block_number: signature.block_number,
                        error: None,
                    },
                )
                .collect(),
        })
    }

    async fn publish(
        &self,
        _: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn create_upload(
        &self,
        _: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn query(&self, _: wire::QueryRequest) -> Result<wire::QueryResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn query_newest(
        &self,
        _: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn get_inbox_ids(
        &self,
        _: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn get_configuration(
        &self,
        _: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn register(
        &self,
        _: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn unregister(
        &self,
        _: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, ApiClientError> {
        unimplemented!("verify only")
    }
    async fn update_subscriptions(
        &self,
        _: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, ApiClientError> {
        unimplemented!("verify only")
    }
}

/// The block number of each verdict, which the backend sets to its input's
/// index.
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
    let backend = VerifyBackend::new();
    let verifier: Arc<Box<dyn SmartContractSignatureVerifier>> =
        Arc::new(Box::new(backend.client(LimitsConfiguration::default())));
    assert_eq!(
        verified_blocks(&verifier, &inputs(SIGNATURES, 65)).await,
        in_order(SIGNATURES)
    );
    assert_eq!(backend.counts(), vec![SIGNATURES]);
}

/// A request ends before the signature whose framed bytes would take it
/// past the published request bytes. At exactly the encoded length of two
/// of the largest signatures, five take requests of two, two and one; one
/// byte less, and each goes alone.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_splits_by_request_bytes() {
    let inputs = inputs(5, MAX_SCW_SIGNATURE_BYTES);
    let two = request(&inputs[..2]).encoded_len();
    for (max_request_bytes, counts) in [(two, vec![2, 2, 1]), (two - 1, vec![1; 5])] {
        let backend = VerifyBackend::new();
        let api = backend.client(LimitsConfiguration {
            max_request_bytes,
            ..LimitsConfiguration::default()
        });
        assert_eq!(verified_blocks(&api, &inputs).await, in_order(5));
        assert_eq!(backend.counts(), counts);
        assert!(
            backend
                .requests
                .lock()
                .unwrap()
                .iter()
                .all(|r| r.1 <= max_request_bytes)
        );
    }
}

/// A published `max_scw_signatures` above what one identity update can carry
/// is held at `MAX_SCW_SIGNATURES_PER_REQUEST`. The requests run four at a
/// time, and their verdicts keep input order although later requests finish
/// first.
// verifies: CONF-073
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_size_is_clamped_to_protocol_maximum() {
    const SIGNATURES: usize = 1000;
    let backend = VerifyBackend::new();
    let api = backend.client(LimitsConfiguration {
        max_scw_signatures: 1000,
        ..LimitsConfiguration::default()
    });
    assert_eq!(
        verified_blocks(&api, &inputs(SIGNATURES, 65)).await,
        in_order(SIGNATURES)
    );
    assert_eq!(backend.counts(), vec![MAX_SCW_SIGNATURES_PER_REQUEST; 5]);
    assert_eq!(backend.most.load(SeqCst), 4);
}

/// A deployment whose limit fell below the snapshot refuses a request as too
/// large. The client sends its halves instead, so every signature still gets
/// its verdict, in order. A single signature's refusal is returned as it is.
// verifies: IDENT-061, CONF-073
#[rstest]
#[case::count(tonic::Code::InvalidArgument)]
#[case::bytes(tonic::Code::OutOfRange)]
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_refused_as_too_large_is_split(#[case] refusal: tonic::Code) {
    let backend = VerifyBackend::refusing_over(10, refusal);
    let api = backend.client(LimitsConfiguration::default());
    assert_eq!(verified_blocks(&api, &inputs(20, 65)).await, in_order(20));
    assert_eq!(backend.counts(), vec![20, 10, 10]);

    let backend = VerifyBackend::refusing_over(0, refusal);
    let api = backend.client(LimitsConfiguration::default());
    let Err(error) = api.verify_many(&inputs(3, 65)).await else {
        panic!("a refused single signature must fail");
    };
    assert!(!error.is_retryable(), "{error}");
    assert_eq!(backend.counts(), vec![3, 1]);
}

/// A rate-limit rejection is never split, whatever its status code.
// verifies: RATE-041
#[rstest]
#[case::count(tonic::Code::InvalidArgument)]
#[case::bytes(tonic::Code::OutOfRange)]
#[xmtp_common::test(unwrap_try = true)]
async fn scw_batch_rate_limited_is_not_split(#[case] refusal: tonic::Code) {
    let mut backend = VerifyBackend::refusing_over(10, refusal);
    backend.rate_limited = true;
    let api = backend.client(LimitsConfiguration::default());
    let Err(error) = api.verify_many(&inputs(20, 65)).await else {
        panic!("a rate-limited request must fail");
    };
    assert!(error.rate_limit_delay().is_some(), "{error}");
    assert_eq!(backend.counts(), vec![20]);
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
        .verify_smart_contract_wallet_signatures(request(&inputs))
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

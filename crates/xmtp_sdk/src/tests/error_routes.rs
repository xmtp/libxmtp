//! Characterization tests that pin the SDK error each public route returns
//! today, so the crate split cannot change a kind, code or retryable flag
//! unnoticed.

use super::*;
use crate::{ErrorCategory, InstallationId};
use xmtp_db::ConnectionExt;

/// A revocation over the identity-update action limit is the app's input on
/// the public revoke call: `InvalidInput`, never retried.
#[xmtp_common::test(unwrap_try = true)]
async fn revoking_too_many_installations_is_invalid_input() {
    let signer = crate::generate_local_signer().await;
    let client = Client::create(signer.clone(), options()).await?;
    let over = xmtp_configuration::MAX_IDENTITY_UPDATE_ACTIONS + 1;
    let ids = (0..over)
        .map(|n| InstallationId::try_from(hex::encode([n as u8; 32])))
        .collect::<Result<Vec<_>, _>>()?;
    let error = client
        .revoke_installations(signer, ids)
        .await
        .expect_err("revoking past the limit fails");
    let XmtpError::InvalidInput(details) = &error else {
        panic!("expected InvalidInput, got {error:?}");
    };
    assert_eq!(details.code, "InvalidInput");
    assert!(matches!(details.category, ErrorCategory::Input));
    assert!(!details.retryable);
    client.end().await?;
}

const QUERY: &str = "/xmtp.backend.v1.QueryService/Query";

/// What the transport does with identity reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IdentityReads {
    Pass,
    /// Fail with a transient transport error.
    Expired,
    /// Answer with no updates, as a backend missing the history.
    Empty,
}

/// Forwards every request to the shared backend except identity reads, which
/// it answers as `reads` says and counts.
struct IdentityReadTransport {
    inner: xmtp_proto::api::ArcClient,
    reads: Arc<parking_lot::Mutex<IdentityReads>>,
    identity_reads: Arc<std::sync::atomic::AtomicUsize>,
}

fn is_identity_read(body: &prost::bytes::Bytes) -> bool {
    use prost::Message;
    xmtp_proto::backend_v1::QueryRequest::decode(body.clone()).is_ok_and(|request| {
        request.queries.iter().any(|query| {
            query.topic.as_ref().and_then(|topic| topic.topic.first())
                == Some(&(xmtp_proto::types::TopicKind::IdentityUpdatesV1 as u8))
        })
    })
}

#[xmtp_common::async_trait]
impl xmtp_proto::api::Client for IdentityReadTransport {
    fn host(&self) -> &str {
        self.inner.host()
    }

    fn has_credential_source(&self) -> bool {
        self.inner.has_credential_source()
    }

    async fn request(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: prost::bytes::Bytes,
    ) -> Result<http::Response<prost::bytes::Bytes>, xmtp_proto::api::ApiClientError> {
        use prost::Message;
        if path.path() != QUERY || !is_identity_read(&body) {
            return self.inner.request(request, path, body).await;
        }
        self.identity_reads.fetch_add(1, Ordering::SeqCst);
        let reads = *self.reads.lock();
        match reads {
            IdentityReads::Pass => self.inner.request(request, path, body).await,
            IdentityReads::Expired => Err(xmtp_proto::api::ApiClientError::Expired(
                xmtp_common::time::Expired,
            )),
            IdentityReads::Empty => Ok(http::Response::new(
                xmtp_proto::backend_v1::QueryResponse {
                    envelopes: vec![],
                    continuation: Some(xmtp_proto::backend_v1::Continuation { has_more: false }),
                }
                .encode_to_vec()
                .into(),
            )),
        }
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: prost::bytes::Bytes,
    ) -> Result<http::Response<xmtp_proto::api::BytesStream>, xmtp_proto::api::ApiClientError> {
        self.inner.stream(request, path, body).await
    }

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, prost::bytes::Bytes>,
    ) -> Result<http::Response<xmtp_proto::api::BytesStream>, xmtp_proto::api::ApiClientError> {
        self.inner.bidi_stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl xmtp_proto::api::IsConnectedCheck for IdentityReadTransport {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

/// The error's variant name, code, category and retryability.
fn group_write_kind(error: XmtpError) -> (String, String, String, bool) {
    let (name, details) = match error {
        XmtpError::Unknown(details) => ("Unknown", details),
        XmtpError::RequestTimeout(details) => ("RequestTimeout", details),
        XmtpError::Storage(details) => ("Storage", details),
        XmtpError::IdentityNotFound(details) => ("IdentityNotFound", details),
        XmtpError::InvalidArgument(details) => ("InvalidArgument", details),
        other => panic!("unexpected error {other:?}"),
    };
    let category = format!("{:?}", details.category);
    (name.into(), details.code, category, details.retryable)
}

/// Group writes that must load a new member's identity keep the SDK error
/// kind, code, category and retryability of the identity failure: a
/// transient identity read, history the backend does not have, and a dropped
/// pool. The other identity-load leaves cannot reach a group write through
/// the SDK: an inbox id is validated when the app constructs it, the SDK has
/// only the remote smart contract wallet verifier, and handle and
/// required-identity failures do not arise from adding members.
#[xmtp_common::test(unwrap_try = true)]
async fn retryable_flag_is_unchanged_for_identity_failures_in_group_writes() {
    use xmtp_proto::api::ToBoxedClient;

    let reads = Arc::new(parking_lot::Mutex::new(IdentityReads::Pass));
    let identity_reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let Some(BackendSource::Options { options: backend }) = options().backend else {
        panic!("test options name backend options");
    };
    let honest = crate::Backend::connect(backend).await?;
    let backend_client = honest.api.inner();
    let transport = IdentityReadTransport {
        inner: backend_client.inner().clone(),
        reads: reads.clone(),
        identity_reads: identity_reads.clone(),
    };
    let api = xmtp_api_backend::BackendClient::new(transport.arced())
        .with_auth_handle(backend_client.auth_handle());
    let path = std::env::temp_dir().join(format!(
        "xmtp-sdk-identity-writes-{}-{}.db3",
        std::process::id(),
        xmtp_common::time::now_ns()
    ));
    let faulty = ClientOptions {
        backend: Some(BackendSource::Connected {
            backend: Arc::new(crate::Backend {
                api: Arc::new(xmtp_api_backend::TrackedStatsClient::new(api)),
                auth_handle: honest.auth_handle,
                options: honest.options,
            }),
        }),
        storage: StorageOptions {
            location: explicit_location(&path),
            ..options().storage
        },
        ..options()
    };
    let alix = Client::create(crate::generate_local_signer().await, faulty).await?;
    let group = alix.conversations().create_group(vec![], None).await?;

    let mut observed = vec![];
    for (leaf, mode) in [
        ("transient read", IdentityReads::Expired),
        ("missing history", IdentityReads::Empty),
        ("dropped pool", IdentityReads::Pass),
    ] {
        let peer = Client::create(crate::generate_local_signer().await, options()).await?;
        let before = identity_reads.load(Ordering::SeqCst);
        *reads.lock() = mode;
        if mode == IdentityReads::Pass {
            alix.inner.context.db().disconnect()?;
        }
        let created = alix
            .conversations()
            .create_group(vec![peer.inbox_id()], None)
            .await
            .map(drop);
        let added = group.add_members(vec![peer.inbox_id()]).await.map(drop);
        if mode == IdentityReads::Pass {
            alix.inner.context.db().reconnect()?;
        } else {
            assert!(
                identity_reads.load(Ordering::SeqCst) > before,
                "{leaf}: no identity read reached the transport"
            );
        }
        *reads.lock() = IdentityReads::Pass;
        for (route, result) in [("create_group", created), ("add_members", added)] {
            let error = result.expect_err("the group write fails");
            observed.push((leaf, route, group_write_kind(error)));
        }
        peer.end().await?;
    }
    let kind = |name: &str, code: &str, category: &str, retryable: bool| {
        (
            name.to_owned(),
            code.to_owned(),
            category.to_owned(),
            retryable,
        )
    };
    let network = kind("Unknown", "Unknown", "Network", true);
    let unknown = kind("Unknown", "Unknown", "Unknown", true);
    let storage = kind("Storage", "Storage", "Storage", true);
    assert_eq!(
        observed,
        vec![
            ("transient read", "create_group", network.clone()),
            ("transient read", "add_members", network),
            ("missing history", "create_group", unknown.clone()),
            ("missing history", "add_members", unknown),
            ("dropped pool", "create_group", storage.clone()),
            ("dropped pool", "add_members", storage),
        ]
    );
    alix.end().await?;
    let _ = std::fs::remove_file(path);
}

//! The instance lookup through a backend that lies about which inbox holds
//! a handle. The SDK client's transport is concrete, so the lie is told one
//! layer down, in the bytes of the `GetInboxIds` response.

use super::*;
use prost::{Message, bytes::Bytes};
use xmtp_proto::{
    api::{
        ApiClientError, ArcClient, BytesStream, Client as Transport, IsConnectedCheck,
        ToBoxedClient,
    },
    backend_v1::GetInboxIdsResponse,
    xmtp::identity::associations::IdentifierKind,
};

const GET_INBOX_IDS: &str = "/xmtp.backend.v1.IdentityService/GetInboxIds";

/// A transport that forwards every request to a real backend, but answers
/// each listed handle with a chosen inbox, as a lying backend would.
struct LyingTransport {
    inner: ArcClient,
    lies: HashMap<String, String>,
}

#[xmtp_common::async_trait]
impl Transport for LyingTransport {
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
        body: Bytes,
    ) -> Result<http::Response<Bytes>, ApiClientError> {
        let lookup = path.path() == GET_INBOX_IDS;
        let response = self.inner.request(request, path, body).await?;
        if !lookup {
            return Ok(response);
        }
        let (parts, body) = response.into_parts();
        let mut answer = GetInboxIdsResponse::decode(body)?;
        for entry in &mut answer.responses {
            if entry.identifier_kind == IdentifierKind::Handle as i32
                && let Some(inbox_id) = self.lies.get(&entry.identifier)
            {
                entry.inbox_id = Some(inbox_id.clone());
            }
        }
        Ok(http::Response::from_parts(
            parts,
            answer.encode_to_vec().into(),
        ))
    }

    async fn stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: Bytes,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.inner.stream(request, path, body).await
    }

    async fn bidi_stream(
        &self,
        request: http::request::Builder,
        path: http::uri::PathAndQuery,
        body: xmtp_common::BoxDynStream<'static, Bytes>,
    ) -> Result<http::Response<BytesStream>, ApiClientError> {
        self.inner.bidi_stream(request, path, body).await
    }
}

#[xmtp_common::async_trait]
impl IsConnectedCheck for LyingTransport {
    async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }
}

/// The shared backend, with lookups that answer `lies`.
async fn lying_backend(lies: HashMap<String, String>) -> Result<BackendSource, XmtpError> {
    let Some(BackendSource::Options { options }) = options().backend else {
        panic!("test options name backend options");
    };
    let honest = crate::Backend::connect(options).await?;
    let backend_client = honest.api.inner();
    let transport = LyingTransport {
        inner: backend_client.inner().clone(),
        lies,
    };
    let api = xmtp_api_backend::BackendClient::new(transport.arced())
        .with_auth_handle(backend_client.auth_handle());
    Ok(BackendSource::Connected {
        backend: Arc::new(crate::Backend {
            api: Arc::new(xmtp_api_backend::TrackedStatsClient::new(api)),
            auth_handle: honest.auth_handle,
            options: honest.options,
        }),
    })
}

// verifies: IDENT-085
#[xmtp_common::test(unwrap_try = true)]
async fn instance_handle_lookup_drops_an_answer_the_inbox_did_not_claim() {
    let bo = Client::create(crate::generate_local_signer().await, options()).await?;
    let caro = Client::create(crate::generate_local_signer().await, options()).await?;
    let (bo_handle, caro_handle) = (random_handle(), random_handle());
    claim_visible(&bo, &bo_handle).await?;
    claim_visible(&caro, &caro_handle).await?;
    let unclaimed = random_handle();
    // The backend names Bo for a handle that no inbox holds, and for Caro's
    // handle. Bo holds a handle, but neither of these.
    let lies = HashMap::from([
        (unclaimed.clone(), bo.inbox_id().into_checked()?),
        (caro_handle.clone(), bo.inbox_id().into_checked()?),
    ]);
    let alix = Client::create(
        crate::generate_local_signer().await,
        ClientOptions {
            backend: Some(lying_backend(lies).await?),
            ..options()
        },
    )
    .await?;

    // The lie reaches Alix: the availability report checks no state.
    let reported = alix.can_message(vec![handle_identity(&unclaimed)]).await?;
    assert!(
        reported[&format!("handle:{unclaimed}")],
        "the backend names an inbox for the unclaimed handle"
    );
    for handle in [&unclaimed, &caro_handle] {
        assert_eq!(
            alix.inbox_id_for(handle_identity(handle)).await?,
            None,
            "a lie about {handle} names no inbox"
        );
    }
    // An honest answer through the same transport still resolves.
    assert_eq!(
        alix.inbox_id_for(handle_identity(&bo_handle)).await?,
        Some(bo.inbox_id())
    );
    alix.end().await?;
    bo.end().await?;
    caro.end().await?;
}

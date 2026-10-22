//! A backend client for commit-log worker tests. It forwards every call to
//! the test backend, except that it refuses commit-log publishes for chosen
//! commit-log ids and answers reads of chosen commit-log topics from a
//! script. It records the commit-log ids and topics of every call.
use crate::context::XmtpSharedContext;
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use xmtp_proto::{
    api::ApiClientError,
    api_client::XmtpBackendClient,
    backend_v1::{self as wire, client_envelope::Payload},
    types::TopicKind,
};

pub(super) type Context = Arc<
    crate::context::XmtpMlsLocalContext<
        ScriptedApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

#[derive(Default)]
pub(super) struct Script {
    /// Commit-log ids whose publishes are refused.
    pub(super) refuse: HashSet<Vec<u8>>,
    /// Every commit-log publish fails as a transport failure would.
    pub(super) unavailable: bool,
    /// Commit-log ids whose next publish request is refused as too large,
    /// so the client splits it. Each id is refused this way once.
    pub(super) too_large_once: HashSet<Vec<u8>>,
    /// Commit-log ids whose publishes are refused after this many accepted
    /// requests.
    pub(super) refuse_after: HashMap<Vec<u8>, usize>,
    /// The accepted publish requests of each commit-log id.
    accepted: HashMap<Vec<u8>, usize>,
    /// Commit-log topics whose reads return these envelopes.
    pub(super) reads: HashMap<Vec<u8>, Vec<wire::ServerEnvelope>>,
    /// The commit-log id of each sealed entry of each publish request.
    pub(super) published: Vec<Vec<Vec<u8>>>,
    /// The commit-log topics of each read request.
    pub(super) read: Vec<Vec<Vec<u8>>>,
}

impl Script {
    /// The publish requests that carried an entry for `commit_log_id`.
    pub(super) fn publishes_for(&self, commit_log_id: &[u8]) -> usize {
        self.published
            .iter()
            .filter(|ids| ids.iter().any(|id| id == commit_log_id))
            .count()
    }
}

#[derive(Clone)]
pub(super) struct ScriptedApi {
    inner: crate::utils::TestClient,
    script: Arc<Mutex<Script>>,
}

fn refused() -> ApiClientError {
    ApiClientError::OtherUnretryable(Box::new(tonic::Status::new(
        tonic::Code::InvalidArgument,
        "refused by the test script",
    )))
}

fn too_large() -> ApiClientError {
    ApiClientError::OtherUnretryable(Box::new(tonic::Status::new(
        tonic::Code::ResourceExhausted,
        "too large for the test script",
    )))
}

fn is_commit_log(topic: &[u8]) -> bool {
    topic.first() == Some(&(TopicKind::CommitLogEntriesV1 as u8))
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for ScriptedApi {
    type Error = ApiClientError;

    fn set_limits(&self, limits: Arc<xmtp_configuration::LimitsConfiguration>) {
        self.inner.set_limits(limits)
    }

    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        let ids: Vec<Vec<u8>> = request
            .envelopes
            .iter()
            .filter_map(|envelope| match &envelope.payload {
                Some(Payload::SealedCommitLogEntry(entry)) => Some(entry.commit_log_id.clone()),
                _ => None,
            })
            .collect();
        if !ids.is_empty() {
            let mut script = self.script.lock();
            if script.unavailable {
                script.published.push(ids);
                return Err(ApiClientError::Expired(xmtp_common::time::Expired));
            }
            if let Some(id) = ids
                .iter()
                .find(|id| script.too_large_once.contains(*id))
                .cloned()
            {
                script.too_large_once.remove(&id);
                script.published.push(ids);
                return Err(too_large());
            }
            let refuse = ids.iter().any(|id| {
                script.refuse.contains(id)
                    || script.refuse_after.get(id).is_some_and(|after| {
                        script.accepted.get(id).copied().unwrap_or(0) >= *after
                    })
            });
            if !refuse {
                for id in ids.iter().collect::<HashSet<_>>() {
                    *script.accepted.entry(id.clone()).or_default() += 1;
                }
            }
            script.published.push(ids);
            if refuse {
                return Err(refused());
            }
        }
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        let topics: Vec<Vec<u8>> = request
            .queries
            .iter()
            .filter_map(|query| query.topic.as_ref().map(|topic| topic.topic.clone()))
            .collect();
        let scripted = {
            let mut script = self.script.lock();
            let commit_log: Vec<_> = topics
                .iter()
                .filter(|t| is_commit_log(t))
                .cloned()
                .collect();
            if !commit_log.is_empty() {
                script.read.push(commit_log);
            }
            (!topics.is_empty() && topics.iter().all(|topic| script.reads.contains_key(topic)))
                .then(|| {
                    request
                        .queries
                        .iter()
                        .flat_map(|query| {
                            let topic = &query.topic.as_ref().expect("a queried topic").topic;
                            let after = query.cursor.as_ref().map_or(0, |c| c.sequence_id);
                            script.reads[topic]
                                .iter()
                                .filter(move |envelope| {
                                    envelope
                                        .meta
                                        .as_ref()
                                        .and_then(|m| m.cursor.as_ref())
                                        .is_some_and(|cursor| cursor.sequence_id > after)
                                })
                                .cloned()
                        })
                        .collect::<Vec<_>>()
                })
        };
        if let Some(envelopes) = scripted {
            return Ok(wire::QueryResponse {
                envelopes,
                continuation: Some(wire::Continuation { has_more: false }),
            });
        }
        self.inner.query(request).await
    }
    async fn query_newest(
        &self,
        request: wire::QueryNewestRequest,
    ) -> Result<wire::QueryNewestResponse, Self::Error> {
        self.inner.query_newest(request).await
    }
    async fn get_inbox_ids(
        &self,
        request: wire::GetInboxIdsRequest,
    ) -> Result<wire::GetInboxIdsResponse, Self::Error> {
        self.inner.get_inbox_ids(request).await
    }
    async fn get_configuration(
        &self,
        request: wire::GetConfigurationRequest,
    ) -> Result<wire::GetConfigurationResponse, Self::Error> {
        self.inner.get_configuration(request).await
    }
    async fn verify_smart_contract_wallet_signatures(
        &self,
        request: wire::VerifySmartContractWalletSignaturesRequest,
    ) -> Result<wire::VerifySmartContractWalletSignaturesResponse, Self::Error> {
        self.inner
            .verify_smart_contract_wallet_signatures(request)
            .await
    }
    async fn register(
        &self,
        request: wire::RegisterRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.register(request).await
    }
    async fn unregister(
        &self,
        request: wire::UnregisterRequest,
    ) -> Result<wire::UnregisterResponse, Self::Error> {
        self.inner.unregister(request).await
    }
    async fn update_subscriptions(
        &self,
        request: wire::UpdateSubscriptionsRequest,
    ) -> Result<wire::RecipientState, Self::Error> {
        self.inner.update_subscriptions(request).await
    }
}

/// The same client, database, and identity as `base`, with its backend
/// calls going through a script. It runs no background workers.
pub(super) async fn scripted(
    base: &crate::utils::FullXmtpClient,
) -> (crate::Client<Context>, Arc<Mutex<Script>>) {
    let script = Arc::new(Mutex::new(Script::default()));
    let api = ScriptedApi {
        inner: base.context.api().api_client.raw_for_test().clone(),
        script: script.clone(),
    };
    let client = crate::builder::ClientBuilder::from_client(base.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .build()
        .await
        .expect("a scripted client");
    (client, script)
}

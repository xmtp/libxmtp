//! Identity-load failures keep the retryability their leaf error gives, on
//! each route that loads an identity: dependency resolution, the installation
//! diff, the incoming receiver's group head and its Welcome head.
//!
//! The leaves are injected at the backend (identity reads), the database (a
//! dropped pool) and the smart contract wallet verifier. A smart contract
//! wallet inbox registered on the local chain makes each route verify a
//! wallet signature.
#![allow(clippy::unwrap_used)]

use super::builder_native_only::{FailingVerifier, register_scw_inbox};
use crate::builder::ClientBuilder;
use crate::context::XmtpSharedContext;
use crate::groups::MlsGroup;
use crate::groups::group_membership::GroupMembership;
use crate::identity_updates::{
    IdentityRequirement, IdentityUpdates, resolve_identity_requirements,
};
use crate::utils::FullXmtpClient;
use crate::worker::NeedsDbReconnect;
use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::ConnectionExt;
use xmtp_db::incoming_envelope::{NetworkEntityKind, StreamTopic};
use xmtp_db::prelude::*;
use xmtp_id::associations::test_utils::WalletTestExt;
use xmtp_id::scw_verifier::VerifierError;
use xmtp_id::utils::test::{SmartWalletContext, docker_smart_wallet};
use xmtp_proto::types::{GroupId, Topic, TopicKind};
use xmtp_proto::{api::ApiClientError, api_client::XmtpBackendClient, backend_v1 as wire};

/// What the backend does with identity reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IdentityReads {
    Pass,
    /// Fail with a transient `UNAVAILABLE` status.
    Unavailable,
    /// Answer with no updates, as a backend missing the history.
    Empty,
}

/// Passes every request to the test backend except identity reads, which it
/// answers as `reads` says and counts.
#[derive(Clone)]
struct IdentityReadFaults {
    inner: crate::utils::TestClient,
    reads: Arc<Mutex<IdentityReads>>,
    identity_reads: Arc<AtomicUsize>,
}

fn is_identity_read(request: &wire::QueryRequest) -> bool {
    request.queries.iter().any(|query| {
        query
            .topic
            .as_ref()
            .and_then(|topic| Topic::parse(&topic.topic).ok())
            .is_some_and(|topic| topic.kind() == TopicKind::IdentityUpdatesV1)
    })
}

impl IdentityReadFaults {
    /// The injected answer to an identity read, or `None` to forward it.
    fn injected(
        &self,
        request: &wire::QueryRequest,
    ) -> Option<Result<wire::QueryResponse, ApiClientError>> {
        if !is_identity_read(request) {
            return None;
        }
        self.identity_reads.fetch_add(1, Ordering::SeqCst);
        match *self.reads.lock() {
            IdentityReads::Pass => None,
            IdentityReads::Unavailable => Some(Err(ApiClientError::client(
                xmtp_api_grpc::error::GrpcError::Status(tonic::Status::unavailable("injected")),
            ))),
            IdentityReads::Empty => Some(Ok(wire::QueryResponse {
                envelopes: vec![],
                continuation: Some(wire::Continuation { has_more: false }),
            })),
        }
    }
}

#[xmtp_common::async_trait]
impl XmtpBackendClient for IdentityReadFaults {
    type Error = ApiClientError;

    fn set_limits(&self, limits: Arc<xmtp_configuration::LimitsConfiguration>) {
        self.inner.set_limits(limits)
    }
    async fn publish(
        &self,
        request: wire::PublishRequest,
    ) -> Result<wire::PublishResponse, Self::Error> {
        self.inner.publish(request).await
    }
    async fn create_upload(
        &self,
        request: wire::CreateUploadRequest,
    ) -> Result<wire::CreateUploadResponse, Self::Error> {
        self.inner.create_upload(request).await
    }
    async fn query(&self, request: wire::QueryRequest) -> Result<wire::QueryResponse, Self::Error> {
        match self.injected(&request) {
            Some(answer) => answer,
            None => self.inner.query(request).await,
        }
    }
    async fn query_raw_envelopes(
        &self,
        request: wire::QueryRequest,
    ) -> Result<xmtp_proto::types::ReceivedQueryResponse, Self::Error> {
        match self.injected(&request) {
            Some(answer) => answer.map(Into::into),
            None => self.inner.query_raw_envelopes(request).await,
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Leaf {
    /// A transient `ApiError` from an identity read.
    TransientApi,
    /// `ApiError::InvalidRequest`, from an inbox id that is not hex.
    InvalidRequest,
    /// A retryable `StorageError`, from a dropped pool.
    Storage,
    /// `SignatureError::VerifierError(NoVerifier)`.
    NoVerifier,
    /// `SignatureError::VerifierError(MalformedEipUrl)`.
    MalformedEipUrl,
    /// `AssociationError::MissingIdentityUpdate`, from a backend missing the
    /// history.
    MissingUpdate,
}

/// What one route returned for one leaf. A receiver head that the leaf left
/// pending reports its stored retry: `retryable` is `!blocked`, `code` is the
/// stored code and `reconnect` is `None`. A head that was settled reports
/// `code` `"settled"`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outcome {
    route: &'static str,
    leaf: Leaf,
    retryable: bool,
    reconnect: Option<bool>,
    code: String,
}

/// The first three variant names of an error's wrapper chain, such as
/// `Client(Api(Api`.
fn chain(error: &impl std::fmt::Debug) -> String {
    format!("{error:?}")
        .split('(')
        .take(3)
        .take_while(|part| part.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .collect::<Vec<_>>()
        .join("(")
}

fn outcome(
    route: &'static str,
    leaf: Leaf,
    error: &(impl RetryableError + std::fmt::Debug),
    reconnect: Option<bool>,
) -> Outcome {
    Outcome {
        route,
        leaf,
        retryable: error.is_retryable(),
        reconnect,
        code: chain(error),
    }
}

/// What a sync entry point returned: `code` is `"ok"` when it succeeded,
/// and otherwise the wrapper chain and the error code, such as
/// `Sync GroupError::Sync`.
fn returned<T, E: RetryableError + ErrorCode + std::fmt::Debug>(
    route: &'static str,
    leaf: Leaf,
    result: &Result<T, E>,
    reconnect: impl Fn(&E) -> bool,
) -> Outcome {
    match result {
        Ok(_) => Outcome {
            route,
            leaf,
            retryable: false,
            reconnect: None,
            code: "ok".into(),
        },
        Err(error) => Outcome {
            code: format!("{} {}", chain(error), error.error_code()),
            ..outcome(route, leaf, error, Some(reconnect(error)))
        },
    }
}

fn pending_outcome(
    route: &'static str,
    leaf: Leaf,
    client: &FullXmtpClient,
    topic: &StreamTopic,
) -> Outcome {
    let pending = client.context.db().first_pending_envelope(topic).unwrap();
    Outcome {
        route,
        leaf,
        retryable: pending.as_ref().is_some_and(|pending| !pending.blocked),
        reconnect: None,
        code: pending.map_or("settled".into(), |pending| {
            pending.error_code.unwrap_or_default()
        }),
    }
}

/// Today's results, pinned at the PR 1 merge commit. `group sync` and
/// `welcome sync` are what those entry points return; `receiver` and
/// `welcome` are the heads they leave. Missing history settles a receiver
/// group head as a safe commit rejection and a Welcome as a terminal failure,
/// and both syncs succeed. `ApiError::InvalidRequest` and a dropped pool reach only
/// the direct routes: a receiver head holds only hex inbox ids, and a dropped
/// pool fails the head's own storage reads before any identity load.
fn expected() -> Vec<Outcome> {
    use Leaf::*;
    let row = |route, leaf, retryable, reconnect, code: &str| Outcome {
        route,
        leaf,
        retryable,
        reconnect,
        code: code.into(),
    };
    vec![
        row(
            "dependency",
            TransientApi,
            true,
            Some(false),
            "Client(Api(Api",
        ),
        row(
            "diff",
            TransientApi,
            true,
            None,
            "IdentityDependency(Client(Api",
        ),
        row(
            "group sync",
            TransientApi,
            true,
            Some(false),
            "Sync GroupError::Sync",
        ),
        row("receiver", TransientApi, true, None, "identity_dependency"),
        row(
            "welcome sync",
            TransientApi,
            true,
            Some(false),
            "StreamBarrier BarrierError::Incomplete",
        ),
        row("welcome", TransientApi, true, None, "identity_dependency"),
        row(
            "dependency",
            InvalidRequest,
            false,
            Some(false),
            "Client(Api(InvalidRequest",
        ),
        row(
            "diff",
            InvalidRequest,
            false,
            None,
            "IdentityDependency(Client(Api",
        ),
        row(
            "dependency",
            Storage,
            true,
            Some(true),
            "Storage(Connection(Platform",
        ),
        row(
            "diff",
            Storage,
            true,
            None,
            "IdentityDependency(Storage(Connection",
        ),
        row(
            "dependency",
            NoVerifier,
            true,
            Some(false),
            "Client(SignatureValidation(VerifierError",
        ),
        row(
            "diff",
            NoVerifier,
            true,
            None,
            "IdentityDependency(Client(SignatureValidation",
        ),
        row(
            "group sync",
            NoVerifier,
            true,
            Some(false),
            "Sync GroupError::Sync",
        ),
        row("receiver", NoVerifier, true, None, "identity_dependency"),
        row(
            "welcome sync",
            NoVerifier,
            true,
            Some(false),
            "StreamBarrier BarrierError::Incomplete",
        ),
        row("welcome", NoVerifier, true, None, "identity_dependency"),
        row(
            "dependency",
            MalformedEipUrl,
            false,
            Some(false),
            "Client(SignatureValidation(VerifierError",
        ),
        row(
            "diff",
            MalformedEipUrl,
            false,
            None,
            "IdentityDependency(Client(SignatureValidation",
        ),
        row(
            "group sync",
            MalformedEipUrl,
            false,
            Some(false),
            "Sync GroupError::Sync",
        ),
        row(
            "receiver",
            MalformedEipUrl,
            false,
            None,
            "identity_dependency",
        ),
        row(
            "welcome sync",
            MalformedEipUrl,
            false,
            Some(false),
            "StreamBarrier BarrierError::Incomplete",
        ),
        row(
            "welcome",
            MalformedEipUrl,
            false,
            None,
            "identity_dependency",
        ),
        row(
            "dependency",
            MissingUpdate,
            false,
            Some(false),
            "MissingReference",
        ),
        row(
            "diff",
            MissingUpdate,
            false,
            None,
            "IdentityDependency(MissingReference",
        ),
        row("group sync", MissingUpdate, false, None, "ok"),
        row("receiver", MissingUpdate, false, None, "settled"),
        row("welcome sync", MissingUpdate, false, None, "ok"),
        row("welcome", MissingUpdate, false, None, "settled"),
    ]
}

type FaultyContext = Arc<
    crate::context::XmtpMlsLocalContext<
        IdentityReadFaults,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// How long a receiver sync waits for a failing head before it returns.
const BARRIER_TIMEOUT: Duration = Duration::from_secs(3);

/// A client of `base`'s inbox and database whose identity reads go through
/// `faults`, verifying smart contract wallets with `verifier` when one is
/// given, giving up on an absent identity reference after `wait`, and ending a
/// receiver sync after [`BARRIER_TIMEOUT`].
async fn faulty_client(
    base: &FullXmtpClient,
    faults: &IdentityReadFaults,
    verifier: Option<FailingVerifier>,
    wait: Duration,
) -> crate::Client<FaultyContext> {
    let mut policy = base.context.incoming_runtime().policy().clone();
    policy.identity_reference_wait = wait;
    policy.barrier_timeout = BARRIER_TIMEOUT;
    let builder = ClientBuilder::from_client(base.clone())
        .api_client(faults.clone())
        .stream_policy(policy)
        .with_disable_workers(true)
        .with_allow_offline(Some(true));
    let builder = match verifier {
        Some(verifier) => builder.with_scw_verifier(verifier),
        None => builder,
    };
    builder.build().await.unwrap()
}

/// Every leaf, read through every route that can reach it. Leaves this test
/// does not inject: `HandleError::NotFound` comes only from handle resolution
/// (`membership_reads_map_missing_updates_and_scw_failures`), and
/// `IdentityError::RequiredIdentityNotFound` only from reading an inbox's
/// creation kind, pinned at the end.
#[rstest::rstest]
#[xmtp_common::timeout(Duration::from_secs(300))]
#[tokio::test]
async fn identity_load_failures_keep_retryability_in_every_wrapper(
    #[future] docker_smart_wallet: SmartWalletContext,
) {
    let (scw, _) = register_scw_inbox(docker_smart_wallet.await).await;
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    crate::identity_updates::load_identity_updates(
        alix.context.api(),
        &alix.context.db(),
        &[scw.as_str()],
    )
    .await
    .unwrap();
    let scw_sequence = alix
        .context
        .db()
        .get_latest_sequence_id(&[scw.as_str()])
        .unwrap()[&scw];
    let wait = Duration::from_millis(200);

    let mut observed = vec![];
    for leaf in [
        Leaf::TransientApi,
        Leaf::InvalidRequest,
        Leaf::Storage,
        Leaf::NoVerifier,
        Leaf::MalformedEipUrl,
        Leaf::MissingUpdate,
    ] {
        crate::tester!(bo, disable_workers, persistent_db);
        let bo = bo.client.clone();
        let group = alix.create_group(None, None).unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();
        bo.sync_welcomes().await.unwrap();
        let welcomed = alix.create_group(None, None).unwrap();
        welcomed
            .add_members(&[bo.inbox_id(), scw.as_str()])
            .await
            .unwrap();
        group.add_members(&[scw.as_str()]).await.unwrap();

        let faults = IdentityReadFaults {
            inner: bo.context.api().api_client.raw_for_test().clone(),
            reads: Arc::new(Mutex::new(IdentityReads::Pass)),
            identity_reads: Default::default(),
        };
        let verifier_calls = Arc::new(AtomicUsize::new(0));
        let verifier = match leaf {
            Leaf::NoVerifier => Some(FailingVerifier::new(
                || VerifierError::NoVerifier("eip155:1".into()),
                verifier_calls.clone(),
            )),
            Leaf::MalformedEipUrl => Some(FailingVerifier::new(
                || VerifierError::MalformedEipUrl,
                verifier_calls.clone(),
            )),
            _ => None,
        };
        let faulty = faulty_client(&bo, &faults, verifier, wait).await;
        *faults.reads.lock() = match leaf {
            Leaf::TransientApi => IdentityReads::Unavailable,
            Leaf::MissingUpdate => IdentityReads::Empty,
            _ => IdentityReads::Pass,
        };
        let inbox_id = match leaf {
            Leaf::InvalidRequest => "not hex".to_string(),
            _ => scw.clone(),
        };
        let requirement = IdentityRequirement {
            inbox_id: inbox_id.clone(),
            sequence_id: scw_sequence as u64,
        };
        if leaf == Leaf::Storage {
            faulty.context.db().disconnect().unwrap();
        }

        let error = resolve_identity_requirements(&faulty.context, &BTreeSet::from([requirement]))
            .await
            .err()
            .unwrap_or_else(|| panic!("{leaf:?}: the dependency resolved"));
        observed.push(outcome(
            "dependency",
            leaf,
            &error,
            Some(error.needs_db_reconnect()),
        ));

        let before = GroupMembership::new();
        let mut after = before.clone();
        after.add(inbox_id, scw_sequence as u64);
        let error = IdentityUpdates::new(&faulty.context)
            .get_installation_diff(
                &faulty.context.db(),
                &GroupId::default(),
                &before,
                &after,
                &before.diff(&after),
            )
            .await
            .err()
            .unwrap_or_else(|| panic!("{leaf:?}: the diff resolved"));
        observed.push(outcome("diff", leaf, &error, None));

        if leaf == Leaf::Storage {
            faulty.context.db().reconnect().unwrap();
        }
        if matches!(leaf, Leaf::InvalidRequest | Leaf::Storage) {
            continue;
        }

        let faulty_group = MlsGroup::new_cached(faulty.context.clone(), &group.group_id)
            .unwrap()
            .0;
        let synced = faulty_group.sync().await;
        observed.push(returned("group sync", leaf, &synced, |error| {
            error.needs_db_reconnect()
        }));
        observed.push(pending_outcome(
            "receiver",
            leaf,
            &bo,
            &StreamTopic::group(group.group_id),
        ));
        let joined = faulty.sync_welcomes().await.map(|groups| groups.len());
        observed.push(returned("welcome sync", leaf, &joined, |error| {
            error.needs_db_reconnect()
        }));
        observed.push(pending_outcome(
            "welcome",
            leaf,
            &bo,
            &StreamTopic {
                entity_id: bo.context.installation_id().to_vec(),
                kind: NetworkEntityKind::Welcome,
            },
        ));

        if leaf == Leaf::MissingUpdate {
            let members = faulty_group.members().await.unwrap();
            assert!(
                members.iter().all(|member| member.inbox_id != scw),
                "the commit adding the wallet was rejected"
            );
            assert_eq!(joined.ok(), Some(0), "the Welcome was not joined");
        }
        match leaf {
            Leaf::NoVerifier | Leaf::MalformedEipUrl => assert!(
                verifier_calls.load(Ordering::SeqCst) > 0,
                "{leaf:?}: the failing verifier was not asked"
            ),
            _ => assert!(
                faults.identity_reads.load(Ordering::SeqCst) > 0,
                "{leaf:?}: no identity read reached the backend"
            ),
        }
    }
    assert_eq!(observed, expected());

    let unknown = generate_local_wallet().identifier().inbox_id(0).unwrap();
    let error = alix
        .inbox_creation_signature_kind(&unknown, true)
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            crate::client::ClientError::Identity(
                crate::identity::IdentityError::RequiredIdentityNotFound
            )
        ),
        "{error:?}"
    );
    assert!(!error.is_retryable());
}

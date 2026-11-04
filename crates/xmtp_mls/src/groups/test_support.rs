//! Engine test support: a real-backend peer whose context has the incoming
//! runtime, the group operations that engine tests share between that peer and
//! a client, a mock context with the engine's features, and the engine
//! internals that tests outside the engine call.
#![allow(clippy::unwrap_used)]

use crate::context::{CoreContext, test_support::MockContext};
use crate::core::events::InternalEvent;
use crate::core::policy::StreamPolicy;
use crate::groups::context::{EngineFeatures, XmtpSharedContext};
use crate::groups::mls_sync::GroupMessageProcessingError;
use crate::groups::welcome_sync::WelcomeService;
#[cfg(all(test, not(target_arch = "wasm32")))]
use crate::identity_updates::IdentityUpdates;
#[cfg(test)]
use xmtp_db::incoming_envelope::{NetworkEntityKind, StreamTopic};
#[cfg(all(test, not(target_arch = "wasm32")))]
use xmtp_id::associations::AssociationState;
pub mod crafted_welcome;
pub mod group_updates;
pub mod message_rows;
pub mod stalled_api;
#[cfg(test)]
mod suite_ops;
pub mod welcome_pointers;
use crate::groups::{
    GroupError, MlsGroup, group_permissions::PolicySet, send_message_opts::SendMessageOpts,
};
use crate::identity_lifecycle::test_support::{PeerApi, PeerContext, PeerFeatures, TestPeer};
use crate::identity_updates::{IdentityFeatures, IdentityResolutionRegistry};
use crate::subscriptions::incoming::{IncomingRuntime, streaming_factory};
use std::sync::Arc;
#[cfg(test)]
pub(crate) use suite_ops::EngineSuiteOps;
use thiserror::Error;
use xmtp_api::{ApiClientWrapper, XmtpApi};
use xmtp_api_backend::envelope::EnvelopeError;
use xmtp_common::RetryableError;
use xmtp_db::{
    group::{ConversationType, GroupQueryArgs},
    group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
    prelude::*,
};
use xmtp_id::AsIdRef;
use xmtp_mls_group_state::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
};
use xmtp_proto::types::{Cursor, GroupId, GroupMessage, Topic};

/// The engine's features: the incoming runtime and identity resolution.
#[derive(Default, Clone)]
pub struct EngineTestFeatures {
    incoming: Arc<IncomingRuntime>,
    identity_resolutions: Arc<IdentityResolutionRegistry>,
}

impl EngineFeatures for EngineTestFeatures {
    fn incoming(&self) -> &IncomingRuntime {
        &self.incoming
    }
}

impl IdentityFeatures for EngineTestFeatures {
    fn identity_resolutions(&self) -> &IdentityResolutionRegistry {
        &self.identity_resolutions
    }
}

impl PeerFeatures<PeerApi> for EngineTestFeatures {
    /// The incoming runtime a client build makes for a streaming API client.
    fn new(
        api: &ApiClientWrapper<PeerApi>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        let factory = streaming_factory(api.api_client.raw_for_test().clone());
        let factory = factory.bind_limits(api.limits()).unwrap_or(factory);
        Self {
            incoming: Arc::new(
                IncomingRuntime::new(policy.clone(), Some(factory))
                    .with_preflight(api.api_client.clone()),
            ),
            identity_resolutions: Arc::new(IdentityResolutionRegistry::starting_after(
                own_inbox_stored_through,
            )),
        }
    }

    fn new_query_only(
        _: &ApiClientWrapper<PeerApi>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        Self::query_only(policy, own_inbox_stored_through)
    }
}

impl EngineTestFeatures {
    /// The features of a context over a query-only API client: no streaming
    /// receiver, as a client build over `ClientBuilder::api_client` has.
    pub(crate) fn query_only(policy: &StreamPolicy, own_inbox_stored_through: Option<i64>) -> Self {
        Self {
            incoming: Arc::new(IncomingRuntime::new(policy.clone(), None)),
            identity_resolutions: Arc::new(IdentityResolutionRegistry::starting_after(
                own_inbox_stored_through,
            )),
        }
    }
}

impl PeerFeatures<Arc<xmtp_api_backend::MockBackendClient>> for EngineTestFeatures {
    fn new(
        _: &ApiClientWrapper<Arc<xmtp_api_backend::MockBackendClient>>,
        policy: &StreamPolicy,
        own_inbox_stored_through: Option<i64>,
    ) -> Self {
        Self::query_only(policy, own_inbox_stored_through)
    }
}

/// A mock context with the engine's features.
pub type MockEngineContext = MockContext<EngineTestFeatures>;

/// A registered installation whose context the engine runs on, over the
/// backend client `A`.
pub type EngineTestPeer<A = PeerApi> = TestPeer<EngineTestFeatures, A>;
/// The context of an `EngineTestPeer`.
pub type EngineTestContext = PeerContext<EngineTestFeatures>;

/// A group on an `EngineTestPeer`'s context.
#[cfg(test)]
pub(crate) type EngineTestGroup = MlsGroup<EngineTestContext>;

/// What a fixture shared by client and peer tests runs on either: the context,
/// a new group of the caller's inbox, and the caller's Welcomes.
#[allow(async_fn_in_trait)]
pub trait EnginePeer {
    /// The context the peer's groups run on.
    type Context: XmtpSharedContext;
    /// The error group creation returns.
    type Error: std::fmt::Debug;

    /// The context the peer runs on.
    fn context(&self) -> &Self::Context;

    /// A new group of the caller's inbox.
    fn create_group(
        &self,
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, Self::Error>;

    /// Processes the caller's new Welcomes and returns the joined groups.
    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self::Context>>, GroupError>;

    /// Sync Welcomes until at least one new group arrives, and return the
    /// new groups. One `sync_welcomes` call can miss a Welcome sent just
    /// before it, because the backend reads the newest Welcome position from
    /// a replica that can lag the primary.
    ///
    /// A `sync_welcomes` error returns at once. Panics if no Welcome arrives
    /// in 20 s.
    #[cfg(test)]
    async fn wait_for_welcomes(&self) -> Result<Vec<MlsGroup<Self::Context>>, GroupError> {
        let poll = async {
            loop {
                let groups = self.sync_welcomes().await?;
                if !groups.is_empty() {
                    return Ok(groups);
                }
                xmtp_common::task::yield_now().await;
            }
        };
        xmtp_common::time::timeout(std::time::Duration::from_secs(20), poll)
            .await
            .expect("no welcome arrived within 20 s")
    }

    /// The local store's key for this installation's Welcome topic.
    #[cfg(test)]
    fn welcome_topic(&self) -> StreamTopic {
        StreamTopic {
            entity_id: self.context().installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        }
    }

    /// Assert that this installation rejected its newest published Welcome
    /// for good: the last terminal rejection names it with `invalid_welcome`
    /// and its pending row is gone.
    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    async fn assert_newest_welcome_rejected(&self) {
        let topic = self.welcome_topic();
        let newest = newest_welcome_cursor(self.context())
            .await
            .expect("a published Welcome");
        let db = self.context().db();
        let rejected = db
            .read_last_rejection(&topic)
            .unwrap()
            .expect("a terminal rejection");
        assert_eq!(rejected.sequence_id, newest);
        assert_eq!(rejected.code, "invalid_welcome");
        assert!(db.pending_envelope(&topic, newest).unwrap().is_none());
    }

    /// Sync Welcomes until the newest published Welcome, which must be past
    /// `after`, is rejected for good; then assert as
    /// [`Self::assert_newest_welcome_rejected`] does and return its cursor.
    /// A Welcome a test just sent can reach the backend's read replica after
    /// the first query. Panics if it is not rejected within 20 s.
    #[cfg(test)]
    #[allow(clippy::unwrap_used)]
    async fn wait_for_newest_welcome_rejected(&self, after: Cursor) -> Cursor {
        let topic = self.welcome_topic();
        let poll = async {
            loop {
                let _ = self.sync_welcomes().await;
                let newest = newest_welcome_cursor(self.context()).await;
                let rejected = self.context().db().read_last_rejection(&topic).unwrap();
                if let (Some(newest), Some(rejected)) = (newest, rejected)
                    && newest > after
                    && rejected.sequence_id == newest
                {
                    return newest;
                }
                xmtp_common::task::yield_now().await;
            }
        };
        let newest = xmtp_common::time::timeout(std::time::Duration::from_secs(20), poll)
            .await
            .expect("the newest Welcome was not rejected within 20 s");
        self.assert_newest_welcome_rejected().await;
        newest
    }
}

/// Group shortcuts that tests share.
#[allow(async_fn_in_trait)]
pub trait MlsGroupExt: Sized {
    /// Add `other`'s inbox to this group.
    async fn invite(
        &self,
        other: &impl EnginePeer,
    ) -> Result<crate::groups::intents::UpdateGroupMembershipResult, GroupError>;

    /// Send `m`, panicking if the send fails.
    async fn send_msg(&self, m: &[u8]);
}

impl<C: XmtpSharedContext> MlsGroupExt for MlsGroup<C> {
    async fn invite(
        &self,
        other: &impl EnginePeer,
    ) -> Result<crate::groups::intents::UpdateGroupMembershipResult, GroupError> {
        self.add_members(&[other.context().inbox_id()]).await
    }

    #[allow(clippy::unwrap_used)]
    async fn send_msg(&self, m: &[u8]) {
        let _ = self.send_message(m, Default::default()).await.unwrap();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
async fn newest_welcome_cursor(context: &impl XmtpSharedContext) -> Option<Cursor> {
    context
        .api()
        .query_welcome_messages(context.installation_id())
        .await
        .unwrap()
        .into_iter()
        .map(|welcome| welcome.cursor)
        .max()
}

impl<A: XmtpApi + 'static> EnginePeer for TestPeer<EngineTestFeatures, A> {
    type Context = PeerContext<EngineTestFeatures, A>;
    type Error = GroupError;

    fn context(&self) -> &Self::Context {
        &self.context
    }

    /// A group with only this installation's inbox, as `Client::create_group`
    /// makes it.
    fn create_group(
        &self,
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError> {
        MlsGroup::create_and_insert(
            self.context.clone(),
            ConversationType::Group,
            policies.unwrap_or_default(),
            opts.unwrap_or_default(),
            None,
        )
    }

    async fn sync_welcomes(&self) -> Result<Vec<MlsGroup<Self::Context>>, GroupError> {
        WelcomeService::new(self.context.clone())
            .sync_welcomes()
            .await
    }
}

/// The client's group operations, run on an `EngineTestPeer`'s context.
#[allow(async_fn_in_trait)]
pub trait EngineTestPeerOps: EnginePeer {
    /// A group stored with its first add of `inbox_ids`, as
    /// `Client::create_group_with_members` makes it.
    async fn create_group_with_members(
        &self,
        inbox_ids: &[impl AsIdRef],
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError>;

    /// The active DM with `inbox_id`, or a new one with the peer added, as
    /// `Client::find_or_create_dm` returns it.
    async fn find_or_create_dm(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError>;

    /// The stored group with `group_id`.
    fn group(&self, group_id: &GroupId) -> Result<MlsGroup<Self::Context>, GroupError>;

    /// The stored groups that match `args`.
    fn find_groups(&self, args: GroupQueryArgs)
    -> Result<Vec<MlsGroup<Self::Context>>, GroupError>;
}

impl<A: XmtpApi + 'static> EngineTestPeerOps for TestPeer<EngineTestFeatures, A> {
    /// A group stored with its first add of `inbox_ids`, as
    /// `Client::create_group_with_members` makes it.
    async fn create_group_with_members(
        &self,
        inbox_ids: &[impl AsIdRef],
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError> {
        let (group, first_add) =
            MlsGroup::create_with_first_add(&self.context, inbox_ids, policies, opts).await?;
        group.sync_first_add(first_add).await?;
        Ok(group)
    }

    /// The active DM with `inbox_id`, or a new one with the peer added, as
    /// `Client::find_or_create_dm` returns it.
    async fn find_or_create_dm(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError> {
        let inbox_id = inbox_id.as_ref();
        if let Some(group) = MlsGroup::find_active_dm::<GroupError>(&self.context, inbox_id).await?
        {
            return Ok(group);
        }
        let group = MlsGroup::create_allowed_dm(&self.context, inbox_id.to_string(), opts)?;
        group.add_members(&[inbox_id]).await?;
        Ok(group)
    }

    /// The stored group with `group_id`.
    fn group(&self, group_id: &GroupId) -> Result<MlsGroup<Self::Context>, GroupError> {
        Ok(MlsGroup::load_by_id(&self.context, group_id)?)
    }

    /// The stored groups that match `args`.
    fn find_groups(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<MlsGroup<Self::Context>>, GroupError> {
        Ok(MlsGroup::find(&self.context, args)?)
    }
}

pub use crate::groups::mls_ext::commit_without_group_info;
pub use crate::groups::mls_sync::GroupHeadOutcome;

#[track_caller]
/// Assert that `error` reports exactly one blocked obligation on `topic`,
/// processed through `processed`, whose cause carries `code`, and return it.
pub fn assert_blocked_obligation(
    error: &crate::subscriptions::barrier::BarrierError,
    topic: &Topic,
    processed: Cursor,
    code: &str,
) -> crate::subscriptions::barrier::BarrierTopic {
    use crate::subscriptions::barrier::{BarrierCause, BarrierError, BarrierFailure};
    let BarrierError::Incomplete { reason, unfinished } = error;
    assert_eq!(*reason, BarrierFailure::Blocked);
    let [status] = unfinished.as_slice() else {
        panic!("expected one blocked obligation, got {unfinished:?}");
    };
    assert_eq!(&status.topic, topic);
    let target = status
        .target
        .expect("blocked work must have a fixed target");
    assert!(status.received >= target);
    assert_eq!(status.processed, processed);
    assert!(target > processed);
    assert!(!status.inactive);
    assert!(matches!(&status.cause, Some(BarrierCause::Blocked(actual)) if actual == code));
    status.clone()
}

/// `version` with its patch number raised by one, keeping any suffix.
pub fn increment_patch_version(version: &str) -> Option<String> {
    // Split version into numeric part and suffix (if any)
    let (version_part, suffix) = match version.split_once('-') {
        Some((v, s)) => (v, Some(s)),
        None => (version, None),
    };

    // Split numeric version string into components
    let mut parts: Vec<&str> = version_part.split('.').collect();

    // Ensure we have exactly 3 parts (major.minor.patch)
    if parts.len() != 3 {
        return None;
    }

    // Parse the patch number and increment it
    let patch = parts[2].parse::<u32>().ok()?;
    let new_patch = patch + 1;

    // Replace the patch number with the incremented value
    let binding = new_patch.to_string();
    parts[2] = &binding;

    // Join the parts back together with dots and add suffix if it existed
    let new_version = parts.join(".");
    match suffix {
        Some(s) => Some(format!("{}-{}", new_version, s)),
        None => Some(new_version),
    }
}

/// Maximum commit records sent in one group snapshot.
#[cfg(any(test, feature = "diagnostics"))]
pub const SNAPSHOT_COMMIT_LIMIT: i64 = crate::groups::diagnostics::SNAPSHOT_COMMIT_LIMIT;

/// The two member inboxes a canonical DM id names.
pub fn parse_canonical_dm_id(dm_id: Option<&str>) -> Result<DmMembers<String>, GroupError> {
    crate::groups::parse_canonical_dm_id(dm_id)
}

/// Publish `group`'s queued intents, as its own publication does.
pub async fn publish_intents<C: XmtpSharedContext>(group: &MlsGroup<C>) -> Result<(), GroupError> {
    group.publish_intents().await
}

/// Store `message` on `group` as processing an external application
/// message does, with the sender's `super_admin` status.
pub fn store_external_application_message<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    storage: &impl XmtpMlsStorageProvider,
    message: &StoredGroupMessage,
    super_admin: bool,
    event_writer: &impl xmtp_events::EventWriter<InternalEvent>,
) -> Result<(), GroupMessageProcessingError> {
    group.store_external_application_message(storage, message, super_admin, event_writer)
}

/// Store a Restored placeholder for `group_id`, as an archive import does
/// before the group's Welcome arrives.
pub fn insert_restored_placeholder<C: XmtpSharedContext>(
    context: &C,
    group_id: &GroupId,
    conversation_type: ConversationType,
) -> Result<xmtp_db::group::StoredGroup, GroupError> {
    MlsGroup::insert(
        context,
        Some(group_id.as_slice()),
        xmtp_db::group::GroupMembershipState::Restored,
        conversation_type,
        PolicySet::default(),
        GroupMetadataOptions::default(),
        None,
        false,
    )
}

/// The component registry committed in `group`'s dictionary.
pub fn load_component_registry(
    group: &openmls::group::MlsGroup,
) -> Result<
    xmtp_mls_group_state::app_data::component_registry::ComponentRegistry,
    xmtp_mls_group_state::app_data::component_source::ComponentSourceError,
> {
    crate::groups::app_data::load_component_registry(group)
}

/// The OpenMLS group context of `group`'s current epoch.
pub fn load_group_context<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<openmls::group::GroupContext, GroupError> {
    group.load_group_context()
}

/// Raise `group`'s minimum version to this client's version. Production
/// code never does this; the method documents why.
pub async fn update_group_min_version_to_match_self<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<(), GroupError> {
    group.update_group_min_version_to_match_self().await
}

/// The intent data of a proposal that adds `add_inbox_ids` and removes
/// `remove_inbox_ids`.
pub fn propose_member_update_data(
    add_inbox_ids: Vec<String>,
    remove_inbox_ids: Vec<String>,
) -> Result<Vec<u8>, crate::groups::intents::IntentError> {
    crate::groups::intents::ProposeMemberUpdateIntentData::new(add_inbox_ids, remove_inbox_ids)
        .try_into()
}

#[allow(clippy::unwrap_used)]
impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Sign `message` under the group signing key of this group's root, as a
    /// member signs it at preparation. A test that changes a group message
    /// signs it again, so that admission accepts the changed bytes and the
    /// change reaches the receiving client.
    pub fn sign_as_member(&self, message: &mut xmtp_proto::backend_v1::GroupMessage) {
        let keys = self
            .with_group_snapshot(|group| {
                Ok(
                    xmtp_mls_group_state::app_data::group_root::check_group_root(
                        group.extensions(),
                        group.group_id().as_slice(),
                    ),
                )
            })
            .unwrap()
            .expect("a keyed group holds the root of its id");
        xmtp_mls_group_keys::group_keys::sign_group_message(&keys, message)
            .expect("signing passes the self-check");
    }
}

/// The error of a group test helper.
#[derive(Error, Debug)]
pub enum TestError {
    #[error("{0}")]
    Generic(String),
    #[error(transparent)]
    Group(#[from] GroupError),
    /// An error of a caller above the engine, such as a client operation.
    #[error(transparent)]
    Other(xmtp_common::BoxDynError),
    #[error(transparent)]
    Api(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    Envelope(#[from] EnvelopeError),
}

impl RetryableError for TestError {
    fn is_retryable(&self) -> bool {
        true
    }
}

#[allow(clippy::unwrap_used)]
impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    // Sends a message to other group and ensures delivery, returning sent message contents.
    pub async fn test_can_talk_with(&self, other: &Self) -> Result<String, TestError> {
        let msg = xmtp_common::rand_string::<20>();
        self.sync().await?;
        self.send_message(msg.as_bytes(), SendMessageOpts::default())
            .await?;

        // Sync to pull down the message
        other.sync().await?;
        let mut other_msgs = other.find_messages(&MsgQueryArgs::default())?;
        if msg.as_bytes() != other_msgs.pop().unwrap().decrypted_message_bytes {
            return Err(TestError::Generic(
                "Sent message was not received.".to_string(),
            ));
        }

        Ok(msg)
    }

    pub async fn test_get_last_message_from_network(&self) -> Result<GroupMessage, TestError> {
        let mut messages = self
            .context
            .api()
            .query_group_messages(self.group_id)
            .await
            .map_err(xmtp_api::dyn_err)?;

        let last_message = messages
            .pop()
            .ok_or(TestError::Generic("No messages found".to_string()))?;

        Ok(last_message)
    }

    pub async fn test_last_message_bytes(&self) -> Result<Option<Vec<u8>>, TestError> {
        self.sync().await?;
        let last = self
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })?
            .pop();
        Ok(last.map(|m| m.decrypted_message_bytes))
    }
}

/// A key package for this installation, signed with its installation key and
/// naming its inbox, whose key package extensions are exactly `extensions`.
/// Its capabilities advertise both XMTP addressing extensions. Nothing
/// uploads it.
pub fn key_package_with_extensions(
    context: &impl XmtpSharedContext,
    ciphersuite: openmls::prelude::Ciphersuite,
    extensions: Vec<openmls::prelude::Extension>,
) -> Vec<u8> {
    use openmls::prelude::{
        ApplicationIdExtension, Capabilities, CredentialWithKey, Extension, ExtensionType,
        Extensions, KeyPackage, LeafNode, ProposalType, tls_codec::Serialize,
    };
    use xmtp_configuration::{
        WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID, WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID,
    };
    let identity = context.identity();
    let capabilities = Capabilities::new(
        None,
        Some(&[ciphersuite]),
        Some(&[
            ExtensionType::ApplicationId,
            ExtensionType::AppDataDictionary,
            ExtensionType::Unknown(WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID),
            ExtensionType::Unknown(WELCOME_POINTEE_ENCRYPTION_AEAD_TYPES_EXTENSION_ID),
        ]),
        Some(&[
            ProposalType::GroupContextExtensions,
            ProposalType::AppDataUpdate,
        ]),
        None,
    );
    KeyPackage::builder()
        .leaf_node_capabilities(capabilities)
        .leaf_node_extensions(
            Extensions::<LeafNode>::single(Extension::ApplicationId(ApplicationIdExtension::new(
                context.inbox_id().as_bytes(),
            )))
            .unwrap(),
        )
        .key_package_extensions(Extensions::from_vec(extensions).unwrap())
        .mark_as_last_resort()
        .build(
            ciphersuite,
            &context.mls_provider(),
            &identity.installation_keys,
            CredentialWithKey {
                credential: identity.credential(),
                signature_key: identity.installation_keys.public_slice().into(),
            },
        )
        .unwrap()
        .key_package()
        .tls_serialize_detached()
        .unwrap()
}

/// The association state of `inbox_id` after `peer` loads its updates.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) async fn get_association_state(
    peer: &EngineTestPeer,
    inbox_id: &str,
) -> AssociationState {
    let conn = peer.context.db();
    crate::identity_updates::load_identity_updates(peer.context.api(), &conn, &[inbox_id])
        .await
        .unwrap();

    IdentityUpdates::new(&peer.context)
        .get_association_state(&conn, inbox_id, None)
        .await
        .unwrap()
}

/// Whether stream recovery treats `error` as a terminal source, which ends
/// recovery at once instead of after its retries.
pub fn terminal_source(error: &(dyn std::error::Error + 'static)) -> bool {
    crate::subscriptions::recovery::terminal_source(error)
}

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
use crate::groups::welcome_sync::{GroupSyncSummary, WelcomeService};
use crate::identity::Identity;
use crate::identity_updates::IdentityUpdates;
use crate::mls_store::MlsStore;
use std::collections::HashMap;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::consent_record::ConsentState;
use xmtp_db::incoming_envelope::{NetworkEntityKind, StreamTopic};
use xmtp_id::InboxOwner;
use xmtp_id::associations::Identifier;
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2};
use xmtp_mls_core::version::VersionInfo;
pub mod crafted_welcome;
use crate::groups::{
    GroupError, MlsGroup, group_permissions::PolicySet, send_message_opts::SendMessageOpts,
};
use crate::identity_lifecycle::test_support::{PeerApi, PeerContext, PeerFeatures, TestPeer};
use crate::identity_updates::{IdentityFeatures, IdentityResolutionRegistry};
use crate::subscriptions::incoming::{IncomingRuntime, streaming_factory};
use std::sync::Arc;
use thiserror::Error;
use xmtp_api::ApiClientWrapper;
use xmtp_api_backend::envelope::EnvelopeError;
use xmtp_common::RetryableError;
use xmtp_db::{
    group::{ConversationType, GroupMembershipState, GroupQueryArgs},
    group_message::{GroupMessageKind, MsgQueryArgs, StoredGroupMessage},
    prelude::*,
};
use xmtp_id::AsIdRef;
use xmtp_mls_group_state::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
};
use xmtp_proto::types::{Cursor, GroupId, GroupMessage};

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
}

/// A mock context with the engine's features.
pub type MockEngineContext = MockContext<EngineTestFeatures>;

/// A registered installation whose context the engine runs on.
pub type EngineTestPeer = TestPeer<EngineTestFeatures>;
/// The context of an `EngineTestPeer`.
pub type EngineTestContext = PeerContext<EngineTestFeatures>;

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
    fn welcome_topic(&self) -> StreamTopic {
        StreamTopic {
            entity_id: self.context().installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        }
    }

    /// Assert that this installation rejected its newest published Welcome
    /// for good: the last terminal rejection names it with `invalid_welcome`
    /// and its pending row is gone.
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

impl EnginePeer for EngineTestPeer {
    type Context = EngineTestContext;
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
pub trait EngineTestPeerOps {
    /// A group stored with its first add of `inbox_ids`, as
    /// `Client::create_group_with_members` makes it.
    async fn create_group_with_members(
        &self,
        inbox_ids: &[impl AsIdRef],
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<EngineTestContext>, GroupError>;

    /// The active DM with `inbox_id`, or a new one with the peer added, as
    /// `Client::find_or_create_dm` returns it.
    async fn find_or_create_dm(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<EngineTestContext>, GroupError>;

    /// The stored group with `group_id`.
    fn group(&self, group_id: &GroupId) -> Result<MlsGroup<EngineTestContext>, GroupError>;

    /// The stored groups that match `args`.
    fn find_groups(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<MlsGroup<EngineTestContext>>, GroupError>;

    /// The database queries of this installation.
    fn db(&self) -> <<EngineTestContext as CoreContext>::Db as xmtp_db::XmtpDb>::DbQuery;

    /// The inbox owner's identifier.
    fn identifier(&self) -> Identifier;

    /// This installation's identity.
    fn identity(&self) -> &Identity;

    /// The identity updates of this installation's context.
    fn identity_updates(&self) -> IdentityUpdates<&EngineTestContext>;

    /// The client's version.
    fn version_info(&self) -> &VersionInfo;

    /// The MLS store of this installation's context.
    fn mls_store(&self) -> MlsStore<EngineTestContext>;

    /// This installation's database, serialized.
    fn db_snapshot(&self) -> Vec<u8>;

    /// Sync every new Welcome, then every active group, as
    /// `Client::sync_all_welcomes_and_groups` does.
    async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError>;

    /// Upload a new key package replacing the current one, as
    /// `Client::rotate_and_upload_key_package` does.
    async fn rotate_and_upload_key_package(&self) -> Result<(), GroupError>;

    /// The current key package of each of `installation_ids` on the network.
    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        GroupError,
    >;

    /// Creates a DM with `other`, sends a message and checks its delivery,
    /// returning the DM and the message, as `Client::test_talk_in_dm_with`
    /// does.
    async fn test_talk_in_dm_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<EngineTestContext>, String), TestError>;

    /// Creates a group with `other`, sends a message and checks its
    /// delivery, returning the group and the message, as
    /// `Client::test_talk_in_new_group_with` does.
    async fn test_talk_in_new_group_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<EngineTestContext>, String), TestError>;
}

impl EngineTestPeerOps for EngineTestPeer {
    /// A group stored with its first add of `inbox_ids`, as
    /// `Client::create_group_with_members` makes it.
    async fn create_group_with_members(
        &self,
        inbox_ids: &[impl AsIdRef],
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<EngineTestContext>, GroupError> {
        let opts = opts.unwrap_or_default();
        let draft = MlsGroup::<EngineTestContext>::prepare_new(
            &self.context,
            None,
            ConversationType::Group,
            policies.unwrap_or_default(),
            &opts,
            None,
        )?;
        let ids = inbox_ids.iter().map(AsIdRef::as_ref).collect::<Vec<_>>();
        let add = MlsGroup::<EngineTestContext>::initial_add(&self.context, &draft, &ids).await?;
        let (stored, intent) = MlsGroup::<EngineTestContext>::store_new(
            &self.context,
            draft,
            GroupMembershipState::Allowed,
            ConversationType::Group,
            opts,
            true,
            add,
        )?;
        let group = MlsGroup::new(
            self.context.clone(),
            stored.id,
            stored.dm_id,
            stored.conversation_type,
            stored.created_at_ns,
        );
        let Some(intent) = intent else {
            return Ok(group);
        };
        match group.sync_until_intent_resolved(intent.id).await {
            Err(error) if !group.intent_pending(intent.id)? => return Err(error),
            Err(error) => tracing::warn!(?error, "first add pending; a sync finishes it"),
            Ok(_) => {}
        }
        Ok(group)
    }

    /// The active DM with `inbox_id`, or a new one with the peer added, as
    /// `Client::find_or_create_dm` returns it.
    async fn find_or_create_dm(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<EngineTestContext>, GroupError> {
        let inbox_id = inbox_id.as_ref();
        let stored = self.context.db().find_active_dm_group(&DmMembers {
            member_one_inbox_id: self.inbox_id(),
            member_two_inbox_id: inbox_id,
        })?;
        if let Some(stored) = stored {
            let group = MlsGroup::new(
                self.context.clone(),
                stored.id,
                stored.dm_id,
                stored.conversation_type,
                stored.created_at_ns,
            );
            group.add_missing_dm_peer().await?;
            return Ok(group);
        }
        let group = MlsGroup::create_dm_and_insert(
            &self.context,
            GroupMembershipState::Allowed,
            inbox_id.to_string(),
            GroupMetadataOptions {
                message_disappearing_settings: opts
                    .unwrap_or_default()
                    .message_disappearing_settings,
                ..Default::default()
            },
            None,
        )?;
        group.add_members(&[inbox_id]).await?;
        Ok(group)
    }

    /// The stored group with `group_id`.
    fn group(&self, group_id: &GroupId) -> Result<MlsGroup<EngineTestContext>, GroupError> {
        Ok(MlsGroup::load_by_id(&self.context, group_id)?)
    }

    /// The stored groups that match `args`.
    fn find_groups(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<MlsGroup<EngineTestContext>>, GroupError> {
        Ok(MlsGroup::find(&self.context, args)?)
    }

    fn db(&self) -> <<EngineTestContext as CoreContext>::Db as xmtp_db::XmtpDb>::DbQuery {
        self.context.db()
    }

    fn identifier(&self) -> Identifier {
        self.owner.get_identifier().expect("the owner's identifier")
    }

    fn identity(&self) -> &Identity {
        self.context.identity()
    }

    fn identity_updates(&self) -> IdentityUpdates<&EngineTestContext> {
        IdentityUpdates::new(&self.context)
    }

    fn version_info(&self) -> &VersionInfo {
        self.context.version_info()
    }

    fn mls_store(&self) -> MlsStore<EngineTestContext> {
        MlsStore::new(self.context.clone())
    }

    fn db_snapshot(&self) -> Vec<u8> {
        use xmtp_db::ConnectionExt;
        self.context
            .db()
            .raw_query(|conn| Ok(conn.serialize_database_to_buffer().to_vec()))
            .expect("a serialized database")
    }

    async fn sync_all_welcomes_and_groups(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, GroupError> {
        WelcomeService::new(self.context.clone())
            .sync_all_welcomes_and_groups(consent_states)
            .await
    }

    async fn rotate_and_upload_key_package(&self) -> Result<(), GroupError> {
        crate::identity_lifecycle::rotate_and_upload_key_package(
            self.context.identity(),
            self.context.api(),
            self.context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .await?;
        crate::worker::key_package_maintenance::nudge_deletion(&self.context)?;
        Ok(())
    }

    async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<
        HashMap<Vec<u8>, Result<VerifiedKeyPackageV2, KeyPackageVerificationError>>,
        GroupError,
    > {
        Ok(self
            .mls_store()
            .get_key_packages_for_installation_ids(installation_ids)
            .await?)
    }

    async fn test_talk_in_dm_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<EngineTestContext>, String), TestError> {
        self.sync_welcomes().await?;
        let dm = self.find_or_create_dm(other.inbox_id(), None).await?;

        other.sync_welcomes().await?;
        let other_dm = other.find_or_create_dm(self.inbox_id(), None).await?;
        other_dm.update_consent_state(ConsentState::Allowed)?;

        assert_eq!(dm.group_id, other_dm.group_id);

        let msg = dm.test_can_talk_with(&other_dm).await?;

        Ok((dm, msg))
    }

    async fn test_talk_in_new_group_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<EngineTestContext>, String), TestError> {
        self.sync_welcomes().await?;
        let group = self
            .create_group_with_members(&[other.inbox_id()], None, None)
            .await?;

        other.sync_welcomes().await?;
        let other_group = other.group(&group.group_id)?;

        assert_eq!(group.group_id, other_group.group_id);

        let msg = group.test_can_talk_with(&other_group).await?;

        Ok((group, msg))
    }
}

pub use crate::groups::mls_ext::commit_without_group_info;
pub use crate::groups::mls_sync::GroupHeadOutcome;

/// Maximum commit records sent in one group snapshot.
#[cfg(any(test, feature = "diagnostics"))]
pub const SNAPSHOT_COMMIT_LIMIT: i64 = crate::groups::diagnostics::SNAPSHOT_COMMIT_LIMIT;

/// The two member inboxes a canonical DM id names.
pub fn parse_canonical_dm_id(dm_id: Option<&str>) -> Result<DmMembers<String>, GroupError> {
    crate::groups::parse_canonical_dm_id(dm_id)
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

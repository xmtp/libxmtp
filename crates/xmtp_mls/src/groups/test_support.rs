//! Engine test support: a real-backend peer whose context has the incoming
//! runtime, the group operations that engine tests share between that peer and
//! a client, and a mock context with the engine's features.

use crate::context::{CoreContext, test_support::MockContext};
use crate::core::policy::StreamPolicy;
use crate::groups::context::{EngineFeatures, XmtpSharedContext};
use crate::groups::welcome_sync::WelcomeService;
use crate::groups::{GroupError, MlsGroup, group_permissions::PolicySet};
use crate::identity_lifecycle::test_support::{PeerApi, PeerContext, PeerFeatures, TestPeer};
use crate::identity_updates::{IdentityFeatures, IdentityResolutionRegistry};
use crate::subscriptions::incoming::{IncomingRuntime, streaming_factory};
use std::sync::Arc;
use xmtp_api::ApiClientWrapper;
use xmtp_db::{
    group::{ConversationType, GroupMembershipState, GroupQueryArgs},
    prelude::*,
};
use xmtp_id::AsIdRef;
use xmtp_mls_group_state::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
};
use xmtp_proto::types::GroupId;

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
}

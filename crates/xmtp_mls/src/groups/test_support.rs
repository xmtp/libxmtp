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
use xmtp_mls_common::{
    group::{DMMetadataOptions, GroupMetadataOptions},
    group_metadata::DmMembers,
};
use xmtp_proto::types::GroupId;

/// The engine's features: the incoming runtime and identity resolution.
#[derive(Default, Clone)]
pub(crate) struct EngineTestFeatures {
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
pub(crate) type MockEngineContext = MockContext<EngineTestFeatures>;

/// A registered installation whose context the engine runs on.
pub(crate) type EngineTestPeer = TestPeer<EngineTestFeatures>;
pub(crate) type EngineTestContext = PeerContext<EngineTestFeatures>;

/// What a fixture shared by client and peer tests runs on either: the context,
/// a new group of the caller's inbox, and the caller's Welcomes.
pub(crate) trait EnginePeer {
    type Context: XmtpSharedContext;
    type Error: std::fmt::Debug;

    fn context(&self) -> &Self::Context;

    fn create_group(
        &self,
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, Self::Error>;

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

impl EngineTestPeer {
    /// A group stored with its first add of `inbox_ids`, as
    /// `Client::create_group_with_members` makes it.
    pub(crate) async fn create_group_with_members(
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
    pub(crate) async fn find_or_create_dm(
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

    pub(crate) fn group(
        &self,
        group_id: &GroupId,
    ) -> Result<MlsGroup<EngineTestContext>, GroupError> {
        Ok(MlsGroup::load_by_id(&self.context, group_id)?)
    }

    pub(crate) fn find_groups(
        &self,
        args: GroupQueryArgs,
    ) -> Result<Vec<MlsGroup<EngineTestContext>>, GroupError> {
        Ok(MlsGroup::find(&self.context, args)?)
    }
}

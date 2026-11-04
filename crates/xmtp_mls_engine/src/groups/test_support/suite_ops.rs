//! The peer operations that only the engine's own suites call.

use super::*;
use crate::groups::welcome_sync::GroupSyncSummary;
use std::collections::HashMap;
use xmtp_configuration::CREATE_PQ_KEY_PACKAGE_EXTENSION;
use xmtp_db::consent_record::ConsentState;
use xmtp_id::InboxOwner;
use xmtp_id::associations::AssociationState;
use xmtp_id::associations::Identifier;
use xmtp_id::key_package::{KeyPackageVerificationError, VerifiedKeyPackageV2};
use xmtp_mls_core::identity::Identity;
use xmtp_mls_core::mls_store::MlsStore;
use xmtp_mls_core::version::VersionInfo;
use xmtp_mls_identity::identity_updates::IdentityUpdates;
use xmtp_mls_identity::identity_updates::load_identity_updates_for_client;
use xmtp_mls_identity::inbox_handles::account_targets;

/// The client reads and operations that only the engine's own suites run on
/// an `EngineTestPeer`'s context.
#[allow(async_fn_in_trait)]
pub(crate) trait EngineSuiteOps: EngineTestPeerOps {
    /// The database queries of this installation.
    fn db(&self) -> <<Self::Context as CoreContext>::Db as xmtp_db::XmtpDb>::DbQuery;

    /// The inbox owner's identifier.
    fn identifier(&self) -> Identifier;

    /// This installation's identity.
    fn identity(&self) -> &Identity;

    /// The identity updates of this installation's context.
    fn identity_updates(&self) -> IdentityUpdates<&Self::Context>;

    /// The client's version.
    fn version_info(&self) -> &VersionInfo;

    /// The MLS store of this installation's context.
    fn mls_store(&self) -> MlsStore<Self::Context>;

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

    /// A group stored with its first add of the inboxes that hold
    /// `identifiers`, as `Client::create_group_with_identifiers` makes it.
    async fn create_group_with_identifiers(
        &self,
        identifiers: &[Identifier],
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError>;

    /// This inbox's association state, loaded from the network first when
    /// `refresh_from_network` is set, as `Client::inbox_state` reads it.
    async fn inbox_state(&self, refresh_from_network: bool)
    -> Result<AssociationState, GroupError>;

    /// Creates a DM with `other`, sends a message and checks its delivery,
    /// returning the DM and the message, as `Client::test_talk_in_dm_with`
    /// does.
    async fn test_talk_in_dm_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<Self::Context>, String), TestError>;

    /// Creates a group with `other`, sends a message and checks its
    /// delivery, returning the group and the message, as
    /// `Client::test_talk_in_new_group_with` does.
    async fn test_talk_in_new_group_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<Self::Context>, String), TestError>;
}

impl<A: XmtpApi + 'static> EngineSuiteOps for TestPeer<EngineTestFeatures, A> {
    fn db(&self) -> <<Self::Context as CoreContext>::Db as xmtp_db::XmtpDb>::DbQuery {
        self.context.db()
    }

    fn identifier(&self) -> Identifier {
        self.owner.get_identifier().expect("the owner's identifier")
    }

    fn identity(&self) -> &Identity {
        self.context.identity()
    }

    fn identity_updates(&self) -> IdentityUpdates<&Self::Context> {
        IdentityUpdates::new(&self.context)
    }

    fn version_info(&self) -> &VersionInfo {
        self.context.version_info()
    }

    fn mls_store(&self) -> MlsStore<Self::Context> {
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
        xmtp_mls_identity::identity_lifecycle::rotate_and_upload_key_package(
            self.context.identity(),
            self.context.api(),
            self.context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .await?;
        xmtp_mls_identity::key_package_maintenance::nudge_deletion(&self.context)?;
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

    async fn create_group_with_identifiers(
        &self,
        identifiers: &[Identifier],
        policies: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Self::Context>, GroupError> {
        let inbox_ids = crate::groups::targets::target_inboxes::<_, GroupError>(
            &self.context,
            &account_targets(identifiers),
        )
        .await?;
        self.create_group_with_members(&inbox_ids, policies, opts)
            .await
    }

    async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<AssociationState, GroupError> {
        let conn = self.context.db();
        let inbox_id = self.inbox_id();
        if refresh_from_network {
            load_identity_updates_for_client(&self.context, &conn, &[inbox_id]).await?;
        }
        Ok(IdentityUpdates::new(&self.context)
            .get_association_state(&conn, inbox_id, None)
            .await?)
    }

    async fn test_talk_in_dm_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<Self::Context>, String), TestError> {
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
    ) -> Result<(MlsGroup<Self::Context>, String), TestError> {
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

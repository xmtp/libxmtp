#![allow(unused)]

use xmtp_api::XmtpApi;
use xmtp_db::XmtpDb;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

use crate::{
    groups::{DMMetadataOptions, GroupError, MlsGroup},
    Client,
};

// Please ensure that all public functions defined in this module
// start with `test_`
impl<ApiClient, Db> Client<ApiClient, Db>
where
    ApiClient: XmtpApi,
    Db: XmtpDb + Send + Sync,
{
    /// Creates a DM with the other client, sends a message, and ensures delivery,
    /// returning the created dm and sent message contents
    pub async fn test_talk_in_dm_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<Self>, String), GroupError> {
        self.sync_welcomes().await?;
        let dm = self
            .find_or_create_dm_by_inbox_id(other.inbox_id(), None)
            .await?;

        other.sync_welcomes().await?;
        let other_dm = other
            .find_or_create_dm_by_inbox_id(self.inbox_id(), None)
            .await?;

        // Since the other client synced welcomes before creating a DM
        // the group_id should be the same.
        assert_eq!(dm.group_id, other_dm.group_id);

        let msg = dm.test_can_talk_with(&other_dm).await?;

        Ok((dm, msg))
    }

    pub async fn test_has_same_sync_group_as(&self, other: &Self) -> Result<(), GroupError> {
        self.sync_welcomes().await?;
        other.sync_welcomes().await?;

        let mut sync_group = self.get_sync_group().await?;
        let mut other_sync_group = other.get_sync_group().await?;
        for i in 0..10 {
            sync_group = self.get_sync_group().await?;
            other_sync_group = other.get_sync_group().await?;

            sync_group.sync().await?;
            other_sync_group.sync().await?;

            if sync_group.group_id == other_sync_group.group_id {
                break;
            }
        }
        assert_eq!(sync_group.group_id, other_sync_group.group_id);

        let epoch = sync_group.epoch().await?;
        let other_epoch = other_sync_group.epoch().await?;
        assert_eq!(epoch, other_epoch);

        let ratchet_tree = sync_group
            .load_mls_group_with_lock(self.mls_provider(), |g| Ok(g.export_ratchet_tree()))?;
        let other_ratchet_tree = other_sync_group
            .load_mls_group_with_lock(other.mls_provider(), |g| Ok(g.export_ratchet_tree()))?;
        assert_eq!(ratchet_tree, other_ratchet_tree);

        Ok(())
    }
}

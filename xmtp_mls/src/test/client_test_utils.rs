#![allow(unused)]

use xmtp_api::XmtpApi;
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;

use crate::{
    groups::{DMMetadataOptions, GroupError, MlsGroup},
    Client,
};

// Please ensure that all public functions defined in this module
// start with `test_`
impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    /// Creates a DM with the other client, sends a message, and ensures delivery,
    /// returning the created dm and sent message contents
    pub async fn test_talk_in_dm_with(
        &self,
        other: &Self,
    ) -> Result<(MlsGroup<Self>, String), GroupError> {
        self.sync_welcomes(&self.mls_provider()?).await?;
        let dm = self
            .find_or_create_dm_by_inbox_id(other.inbox_id(), DMMetadataOptions::default())
            .await?;

        other.sync_welcomes(&other.mls_provider()?).await?;
        let other_dm = other
            .find_or_create_dm_by_inbox_id(self.inbox_id(), DMMetadataOptions::default())
            .await?;

        // Since the other client synced welcomes before creating a DM
        // the group_id should be the same.
        assert_eq!(dm.group_id, other_dm.group_id);

        let msg = dm.test_can_talk_with(&other_dm).await?;

        Ok((dm, msg))
    }

    pub async fn test_has_same_sync_group_as(&self, other: &Self) -> Result<(), GroupError> {
        self.sync_welcomes(&self.mls_provider()?).await?;
        other.sync_welcomes(&other.mls_provider()?).await?;

        let sync_group = self.get_sync_group(&self.mls_provider()?).await?;
        let other_sync_group = other.get_sync_group(&other.mls_provider()?).await?;

        sync_group.sync().await?;
        other_sync_group.sync().await?;

        assert_eq!(sync_group.group_id, other_sync_group.group_id);

        let epoch = sync_group.epoch(&self.mls_provider()?).await?;
        let other_epoch = other_sync_group.epoch(&other.mls_provider()?).await?;
        assert_eq!(epoch, other_epoch);

        let ratchet_tree = sync_group
            .load_mls_group_with_lock(&self.mls_provider()?, |g| Ok(g.export_ratchet_tree()))?;
        let other_ratchet_tree = other_sync_group
            .load_mls_group_with_lock(&other.mls_provider()?, |g| Ok(g.export_ratchet_tree()))?;
        assert_eq!(ratchet_tree, other_ratchet_tree);

        Ok(())
    }
}

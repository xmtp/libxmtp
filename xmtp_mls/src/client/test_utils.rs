use super::*;
use anyhow::Result;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub async fn test_talk_in_dm_with(&self, other: &Self) -> Result<()> {
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

        dm.test_can_talk_with(&other_dm).await?;

        Ok(())
    }
}

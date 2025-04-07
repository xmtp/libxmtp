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
    pub(crate) async fn test_talk_in_dm_with(
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
}

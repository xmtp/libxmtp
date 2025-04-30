use super::*;
use crate::Client;
use xmtp_db::user_preferences::HmacKey;
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::device_sync::content::user_preference_update;
use xmtp_proto::xmtp::device_sync::content::HmacKeyUpdate as HmacKeyUpdateProto;
use xmtp_proto::xmtp::device_sync::content::{
    device_sync_content::Content as ContentProto, PreferenceUpdates,
    UserPreferenceUpdate as UserPreferenceUpdateProto,
};

pub(crate) mod preference_sync_legacy;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(super) async fn sync(
        &self,
        updates: Vec<UserPreferenceUpdateProto>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), ClientError> {
        self.send_device_sync_message(
            provider,
            ContentProto::PreferenceUpdates(PreferenceUpdates {
                updates: updates.clone(),
            }),
        )
        .await?;

        if let Some(handle) = self.worker_handle() {
            updates.iter().for_each(|update| {
                if let UserPreferenceUpdateProto {
                    update: Some(update),
                } = update
                {
                    match update {
                        user_preference_update::Update::ConsentUpdate(_) => {
                            handle.increment_metric(SyncMetric::ConsentSent)
                        }
                        user_preference_update::Update::HmacKeyUpdate(_) => {
                            handle.increment_metric(SyncMetric::HmacSent)
                        }
                    }
                }
            });
        }

        // TODO: v1 support - remove this on next hammer
        // preference_sync_legacy::UserPreferenceUpdate::v1_sync_across_devices(updates.clone(), self)
        // .await?;

        Ok(())
    }

    pub(crate) async fn cycle_hmac(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), ClientError> {
        tracing::info!("Sending new HMAC key to sync group.");

        self.sync(
            vec![UserPreferenceUpdateProto {
                update: Some(user_preference_update::Update::HmacKeyUpdate(
                    HmacKeyUpdateProto {
                        key: HmacKey::random_key(),
                    },
                )),
            }],
            provider,
        )
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        groups::{device_sync::handle::SyncMetric, scoped_client::ScopedGroupClient},
        utils::{LocalTesterBuilder, Tester},
    };
    use serde::{Deserialize, Serialize};
    use xmtp_db::{
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        user_preferences::StoredUserPreferences,
    };

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_hmac_sync() {
        let amal_a = Tester::builder().with_sync_worker().build().await;
        let amal_b = amal_a.builder.build().await;

        amal_a.test_has_same_sync_group_as(&amal_b).await?;

        amal_a.worker().wait(SyncMetric::HmacSent, 1).await?;

        amal_a.sync_device_sync(&amal_a.provider).await?;
        amal_a.worker().wait(SyncMetric::HmacReceived, 1).await?;

        // Wait for a to process the new hmac key
        amal_b
            .get_sync_group(&amal_b.provider)
            .await?
            .sync()
            .await?;
        amal_b.worker().wait(SyncMetric::HmacReceived, 1).await?;

        let pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref())?;
        let pref_b = StoredUserPreferences::load(amal_b.provider.conn_ref())?;

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        amal_a
            .revoke_installations(vec![amal_b.installation_id().to_vec()])
            .await?;

        amal_a.sync_device_sync(&amal_a.provider).await?;
        let new_pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref())?;
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }
}

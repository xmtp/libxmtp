use std::sync::Arc;

use crate::groups::device_sync::DeviceSyncError;
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_db::XmtpOpenMlsProvider;
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

pub async fn upload_debug_package(
    provider: &Arc<XmtpOpenMlsProvider>,
    device_sync_server_url: impl AsRef<str>,
) -> Result<String, DeviceSyncError> {
    let provider = provider.clone();
    let device_sync_server_url = device_sync_server_url.as_ref();

    let options = BackupOptions {
        elements: vec![BackupElementSelection::ClientEvent as i32],
        ..Default::default()
    };

    // Generate a random encryption key
    let key = xmtp_common::rand_vec::<32>();

    // Build the exporter
    let exporter = ArchiveExporter::new(options, provider.clone(), &key);

    let url = format!("{device_sync_server_url}/upload");
    let response = exporter.post_to_url(&url).await?;

    Ok(format!("{response}:{}", hex::encode(key)))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use xmtp_mls_common::group::GroupMetadataOptions;

    use crate::{tester, utils::client_events::upload_debug_package};

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_debug_pkg() {
        tester!(alix, stream);
        tester!(bo);
        tester!(caro);

        let (bo_dm, _msg) = bo.test_talk_in_dm_with(&alix).await?;

        let alix_dm = alix.group(&bo_dm.group_id)?;
        alix_dm.send_message(b"Hello there").await?;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        alix_dm.send_message(b"Hello there").await?;

        let (caro_dm, _) = caro.test_talk_in_dm_with(&alix).await?;
        alix.sync_welcomes().await?;

        let g = alix
            .create_group_with_inbox_ids(
                &[bo.inbox_id().to_string()],
                None,
                GroupMetadataOptions::default(),
            )
            .await?;
        g.update_group_name("Group with the buds".to_string())
            .await?;
        g.send_message(b"Hello there").await?;
        g.sync().await?;

        bo.sync_welcomes().await?;
        let bo_g = bo.group(&g.group_id)?;
        bo_g.send_message(b"Gonna add Caro").await?;
        bo_g.add_members_by_inbox_id(&[caro.inbox_id()]).await?;

        caro.sync_welcomes().await?;
        let caro_g = caro.group(&g.group_id)?;
        caro_g.send_message(b"hi guise!").await?;

        g.sync().await?;

        let key = upload_debug_package(&alix.provider, "http://localhost:5559").await?;
        tracing::info!("{key}");
    }
}

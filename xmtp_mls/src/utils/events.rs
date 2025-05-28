use crate::groups::device_sync::DeviceSyncError;
use serde::Serialize;
use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use xmtp_archive::exporter::ArchiveExporter;
use xmtp_common::time::now_ns;
use xmtp_db::{ConnectionExt, DbConnection, Store};

use xmtp_db::{
    events::{Details, Event, Events},
    XmtpOpenMlsProvider,
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

pub(crate) static EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);

pub(crate) fn track<C: ConnectionExt>(
    db: &DbConnection<C>,
    event: impl AsRef<Event>,
    details: impl Serialize,
    group_id: Option<Vec<u8>>,
) {
    if !EVENTS_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let client_event = event.as_ref();

    let event = match serde_json::to_string(client_event) {
        Ok(event) => event,
        Err(err) => {
            tracing::warn!("ClientEvents: unable to serialize event. {err:?}");
            return;
        }
    };

    let serialized_details = match serde_json::to_value(details) {
        Ok(details) => details,
        Err(err) => {
            tracing::warn!("ClientEvents: unable to serialize details. {err:?}");
            return;
        }
    };

    let event = Events {
        created_at_ns: now_ns(),
        group_id,
        event,
        details: serialized_details,
    };
    if let Err(err) = event.store(db) {
        tracing::warn!("Unable to save event: {err:?}");
    };

    // Clear old events on build.
    if matches!(client_event, Event::ClientBuild) {
        if let Err(err) = Events::clear_old_events(db) {
            tracing::warn!("Unable to clear old events: {err:?}");
        }
    }
}
pub(crate) fn track_err<T, E: Debug, C: ConnectionExt>(
    db: &DbConnection<C>,
    result: Result<T, E>,
    group_id: Option<Vec<u8>>,
) -> Result<T, E> {
    if let Err(err) = &result {
        track(
            db,
            Event::Error,
            Details::Error {
                error: format!("{err:?}"),
            },
            group_id,
        );
    }
    result
}

#[macro_export]
macro_rules! t {
    ($db:expr, $event:expr, $details:expr, $group_id:expr) => {{
        use $crate::utils::events::track;
        track($db, $event, $details, Some($group_id))
    }};
    ($db:expr, $event:expr, $details:expr) => {
        use $crate::utils::events::track;
        track($db, $event, $details, None)
    };
    ($db:expr, $event:expr) => {
        use $crate::utils::events::track;
        track($db, $event, (), None)
    };
}
#[macro_export]
macro_rules! te {
    ($db:expr, $result:expr, $group_id:expr) => {
        use $crate::utils::events::track_err;
        track_err($db, $result, $group_id)
    };
    ($db:expr, $result:expr) => {{
        use $crate::utils::events::track_err;
        track_err($db, $result, None)
    }};
}

pub async fn upload_debug_archive(
    provider: &Arc<XmtpOpenMlsProvider>,
    device_sync_server_url: impl AsRef<str>,
) -> Result<String, DeviceSyncError> {
    let provider = provider.clone();
    let device_sync_server_url = device_sync_server_url.as_ref();

    let options = BackupOptions {
        elements: vec![BackupElementSelection::Event as i32],
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

    use xmtp_db::events::{Event, Events};
    use xmtp_mls_common::group::GroupMetadataOptions;

    use crate::{configuration::DeviceSyncUrls, tester, utils::events::upload_debug_archive};

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_clear_old_events() {
        tester!(alix);

        alix.create_group(None, GroupMetadataOptions::default())?;
        let events = Events::all_events(alix.provider.db())?;
        assert_eq!(events.len(), 2);

        t!(alix.provider.db(), Event::ClientBuild);
        let events = Events::all_events(alix.provider.db())?;
        assert_eq!(events.len(), 1);
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_debug_pkg() {
        tester!(alix, stream, worker);
        tester!(bo);
        tester!(caro);

        let (bo_dm, _msg) = bo.test_talk_in_dm_with(&alix).await?;

        let alix_dm = alix.group(&bo_dm.group_id)?;
        alix_dm.send_message(b"Hello there").await?;
        tokio::time::sleep(Duration::from_millis(1000)).await;
        alix_dm.send_message(b"Hello there").await?;

        caro.test_talk_in_dm_with(&alix).await?;
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

        let k = upload_debug_archive(&alix.provider, DeviceSyncUrls::LOCAL_ADDRESS).await?;
        tracing::info!("{k}");

        // Exported and uploaded no problem
    }
}

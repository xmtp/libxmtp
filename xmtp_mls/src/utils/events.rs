use crate::{groups::device_sync::DeviceSyncError, subscriptions::WorkerEvent};
use parking_lot::RwLock;
use serde::Serialize;
use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
};
use tokio::sync::broadcast;
use xmtp_common::time::now_ns;

use xmtp_archive::exporter::ArchiveExporter;
use xmtp_db::{
    events::{Details, Event, Events},
    XmtpOpenMlsProvider,
};
use xmtp_proto::xmtp::device_sync::{BackupElementSelection, BackupOptions};

pub(crate) static EVENTS_ENABLED: AtomicBool = AtomicBool::new(true);
static WORKER_TX: LazyLock<RwLock<Option<broadcast::Sender<WorkerEvent>>>> =
    LazyLock::new(|| RwLock::default());

pub(crate) fn set_worker_tx(tx: broadcast::Sender<WorkerEvent>) {
    *WORKER_TX.write() = Some(tx);
}

pub(crate) fn track(event: impl AsRef<Event>, details: impl Serialize, group_id: Option<Vec<u8>>) {
    if !EVENTS_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let Some(tx) = (*WORKER_TX.read()).clone() else {
        return;
    };

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
    let _ = tx.send(WorkerEvent::Track(event));

    // Clear old events on build.
    if matches!(client_event, Event::ClientBuild) {
        let _ = tx.send(WorkerEvent::ClearOldEvents);
    }
}
pub(crate) fn track_err<T, E: Debug>(
    result: Result<T, E>,
    group_id: Option<Vec<u8>>,
) -> Result<T, E> {
    if let Err(err) = &result {
        track(
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
    ($event:expr, $details:expr, $group_id:expr) => {{
        use $crate::utils::events::track;
        track($event, $details, Some($group_id))
    }};
    ($event:expr, $details:expr) => {
        use $crate::utils::events::track;
        track($event, $details, None)
    };
    ($event:expr) => {
        use $crate::utils::events::track;
        track($event, (), None)
    };
}
#[macro_export]
macro_rules! te {
    ($result:expr, $group_id:expr) => {
        use $crate::utils::events::track_err;
        track_err($result, $group_id)
    };
    ($result:expr) => {{
        use $crate::utils::events::track_err;
        track_err($result, None)
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

    use crate::{tester, utils::events::upload_debug_archive};

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_clear_old_events() {
        tester!(alix);

        alix.create_group(None, GroupMetadataOptions::default())?;
        let events = Events::all_events(alix.provider.db())?;
        assert_eq!(events.len(), 2);

        t!(Event::ClientBuild);
        let events = Events::all_events(alix.provider.db())?;
        assert_eq!(events.len(), 1);
    }

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

        let k = upload_debug_archive(&alix.provider, "http://localhost:5559").await?;
        tracing::info!("{k}");

        // Exported and uploaded no problem
    }
}

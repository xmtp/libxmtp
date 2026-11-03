//! Facade work that crosses into the notification runner: imports that change
//! what it must keep subscribed.

use crate::context::CoreContext;
use crate::utils::test::notifications;
use crate::worker::device_sync::archive::insert_elements_for_test;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::prelude::*;
use xmtp_proto::xmtp::device_sync::{BackupElement, backup_element::Element};

/// An import that changes consent wakes the notification runner, which
/// removes the topic of the conversation the archive denies.
// verifies: PUSH-260
#[xmtp_common::test(unwrap_try = true)]
async fn archived_consent_wakes_notification_sync() {
    use crate::client::notifications::test_support::{config, task_hash};
    use crate::task_handlers::MlsTaskHandlers;
    use crate::worker::tasks::TaskWorker;
    use xmtp_common::{StreamHandle, time::Duration, time::timeout, wait_for_eq};
    let (client, peer) = notifications::client().await;
    let group = client.create_group(None, None)?;
    client.enable_notifications(config()).await?;
    let mut task_worker = TaskWorker::new(client.context.clone(), MlsTaskHandlers);
    let runner = xmtp_common::spawn(None, async move { task_worker.run().await });
    // Wait until the runner is idle, so only a wake runs it within the hour.
    let idle = || async {
        let tasks = client.db().get_tasks().unwrap();
        let task = tasks
            .iter()
            .find(|task| task.data_hash == task_hash().as_ref());
        let soon = xmtp_common::time::now_ns() + xmtp_common::NS_IN_MIN;
        task.is_some_and(|task| task.next_attempt_at_ns > soon)
    };
    timeout(Duration::from_secs(5), wait_for_eq(idle, true)).await??;
    let topic = xmtp_proto::types::Topic::new_group_message(group.group_id).cloned_vec();
    let holds = || async { peer.state.lock().subscriptions.contains_key(&topic) };
    assert!(holds().await);

    let revision = client.context.task_channels().notification_revision();
    let denied = StoredConsentRecord {
        entity_type: xmtp_db::consent_record::ConsentType::ConversationId,
        state: xmtp_db::consent_record::ConsentState::Denied,
        entity: hex::encode(group.group_id),
        consented_at_ns: xmtp_common::time::now_ns(),
        owed_seq: 0,
    };
    let mut elements = futures::stream::iter([Ok::<_, std::io::Error>(BackupElement {
        element: Some(Element::Consent(denied.into())),
    })]);
    insert_elements_for_test(&mut elements, &client.context).await?;
    assert!(client.context.task_channels().notification_revision() > revision);
    timeout(Duration::from_secs(5), wait_for_eq(holds, false)).await??;
    runner.abort_handle().end();
}

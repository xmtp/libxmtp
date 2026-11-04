//! Device-sync steps that facade and device-sync suites share: consent decided
//! at a set time, a sync round of publish and receive, and the reconcile tasks
//! a client stored.

use crate::worker::device_sync::DeviceSyncError;
use prost::Message as _;
use xmtp_db::{
    StorageError,
    consent_record::{ConsentKey, ConsentState, ConsentValue, ConsentWrite, Owed, write_consent},
    prelude::*,
};
use xmtp_proto::xmtp::mls::database::{Task as TaskProto, task::Task as TaskKindProto};

/// Store `state` at `at` and mark it owed, as an installation whose clock read
/// `at` when the user decided.
pub fn decide_at(
    client: &crate::utils::Tester,
    key: &ConsentKey,
    state: ConsentState,
    at: i64,
) -> Result<(), StorageError> {
    let value = ConsentValue {
        state,
        consented_at_ns: at,
    };
    let write = ConsentWrite::Merge {
        value,
        bound_ns: i64::MAX,
    };
    write_consent(&client.context.db(), key, write, Owed::Mark)?;
    Ok(())
}

/// The value `client` holds for `key`.
pub fn held(client: &crate::utils::Tester, key: &ConsentKey) -> Option<ConsentValue> {
    client
        .context
        .db()
        .consent_row(key)
        .unwrap()
        .map(|row| row.value())
}

/// Read every sync group, then apply what arrived.
pub async fn receive(client: &crate::utils::Tester) -> Result<(), Box<dyn std::error::Error>> {
    client.sync_all_welcomes_and_device_sync_groups().await?;
    client
        .device_sync_client()
        .process_sync_group_messages()
        .await?;
    Ok(())
}

/// Publish everything `client` owes.
pub async fn publish(client: &crate::utils::Tester) -> Result<(), DeviceSyncError> {
    client.device_sync_client().publish_outbox().await
}

/// The group ids of the client's queued add-missing-installations tasks.
pub fn add_task_group_ids(client: &crate::utils::Tester) -> Vec<Vec<u8>> {
    let tasks = client.context.db().get_tasks().unwrap();
    tasks
        .iter()
        .filter_map(|t| match TaskProto::decode(t.data.as_slice()).ok()?.task {
            Some(TaskKindProto::AddMissingInstallations(add)) => Some(add.group_id),
            _ => None,
        })
        .collect()
}

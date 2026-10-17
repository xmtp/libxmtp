//! Shared metadata proposal fixtures.

use super::*;

/// The number of proposals `group` holds pending.
pub(super) fn pending_proposals<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
) -> Result<usize, GroupError> {
    group.with_group_snapshot(|group| Ok(group.pending_proposals().count()))
}

/// A write of `value` to the writer's own display name.
pub(super) fn display_name(value: &str) -> Vec<FieldWrite> {
    vec![FieldWrite {
        component_id: ComponentId::USER_DISPLAY_NAME,
        component_type: ComponentType::TlsMapInboxIdString,
        operation: WriteOperation::SetOwn(value.as_bytes().to_vec()),
    }]
}

/// Queue `writes` by `own` as one intent and publish it, without the
/// checks a public write makes first.
pub(super) async fn publish_writes<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    own: InboxId,
    writes: Vec<FieldWrite>,
) -> Result<(), GroupError> {
    let intent = QueueIntent::app_data_update()
        .data(Vec::<u8>::from(AppDataUpdateIntentData::Fields(
            writes.clone(),
        )))
        .queue(group)?;
    group.publish_field_writes(intent.id, own, &writes).await
}

/// Publish only the proposals of `writes` by `own`, so a member that syncs
/// holds them pending with no commit.
pub(super) async fn publish_proposals<C: XmtpSharedContext>(
    group: &MlsGroup<C>,
    own: InboxId,
    writes: Vec<FieldWrite>,
) -> Result<(), GroupError> {
    let mut payloads = state_write(group.context.mls_storage(), |tx| {
        tx.with_group(group.group_id, |mls_group, storage| {
            let publish = apply_app_data_update_intent(
                storage,
                mls_group,
                AppDataUpdateIntentData::Fields(writes),
                own,
                &[],
                &group.context.identity().installation_keys,
                false,
            )?;
            Ok::<_, GroupError>(Continue(publish.expect("a commit").payloads_to_publish))
        })
    })?
    .into_continued();
    payloads.pop().expect("the commit");
    let messages = group.prepare_group_messages(
        payloads
            .iter()
            .map(|payload| (payload.as_slice(), false))
            .collect(),
    )?;
    group.context.api().send_group_messages(messages).await?;
    Ok(())
}

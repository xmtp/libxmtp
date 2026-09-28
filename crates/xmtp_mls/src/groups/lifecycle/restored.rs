//! Each archive group element commits its inert stub, row, and history together.

use super::*;
use prost::Message;
use xmtp_db::ConnectionExt;
use xmtp_db::restored_group_metadata::StoredRestoredGroupMetadata;
use xmtp_proto::xmtp::device_sync::group_backup::GroupSave;

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    // implements: ARCH-014, ARCH-020, ARCH-021
    pub(crate) fn restore_from_archive(
        context: &Context,
        save: &GroupSave,
    ) -> Result<bool, GroupError> {
        let group_id = GroupId::try_from(save.id.as_slice())?;
        state_write(context.mls_storage(), |tx| {
            if tx.storage().db().find_group(&group_id)?.is_some() {
                return Ok::<_, GroupError>(Continue(merge_activity(
                    &tx.storage().db(),
                    &group_id,
                    save.last_message_ns,
                )?));
            }
            let conversation_type: ConversationType = save.conversation_type().try_into()?;
            let pair = if conversation_type == ConversationType::Dm {
                Some(crate::groups::parse_canonical_dm_id(save.dm_id.as_deref())?)
            } else {
                None
            };
            let attributes = save
                .mutable_metadata
                .as_ref()
                .map(|metadata| &metadata.attributes);
            let attribute = |name| {
                attributes
                    .and_then(|attributes| attributes.get(name))
                    .cloned()
            };
            let opts = GroupMetadataOptions {
                name: attribute("group_name"),
                image_url_square: attribute("group_image_url_square"),
                description: attribute("description"),
                app_data: attribute("app_data"),
                message_disappearing_settings: match (
                    save.message_disappear_from_ns,
                    save.message_disappear_in_ns,
                ) {
                    (Some(from_ns), Some(in_ns)) => {
                        Some(MessageDisappearingSettings::new(from_ns, in_ns))
                    }
                    _ => None,
                },
            };
            let kind = match &pair {
                Some(pair) => InitialGroupKind::RestoredDm {
                    member_one_inbox_id: &pair.member_one_inbox_id,
                    member_two_inbox_id: &pair.member_two_inbox_id,
                },
                None => InitialGroupKind::Group {
                    conversation_type,
                    oneshot_message: None,
                },
            };
            let policy = if pair.is_some() {
                PolicySet::new_dm()
            } else {
                PolicySet::default()
            };
            // The placeholder never receives archived admin lists or a signer.
            let dictionary = initial_dictionary(
                kind,
                &policy
                    .to_proto()
                    .map_err(group_permissions::GroupMutablePermissionsError::from)
                    .map_err(MetadataPermissionsError::from)?,
                &opts,
                context.inbox_id(),
                None,
            )
            .map_err(app_data::migration::BootstrapSynthesisError::from)?;
            let config = build_group_config(dictionary)?;
            if let Some(pair) = &pair {
                Self::insert_dm_row(
                    context,
                    tx,
                    GroupMembershipState::Restored,
                    pair,
                    &opts,
                    Some(&save.id),
                    &config,
                    false,
                )?;
            } else {
                Self::insert_group_row(
                    context,
                    tx,
                    Some(&save.id),
                    GroupMembershipState::Restored,
                    conversation_type,
                    &opts,
                    &config,
                    false,
                )?;
            }
            let storage = tx.storage();
            let db = storage.db();
            use xmtp_db::diesel::prelude::*;
            use xmtp_db::schema::groups::dsl;
            db.raw_query(|conn| {
                xmtp_db::diesel::update(dsl::groups.find(&group_id))
                    .set((
                        dsl::created_at_ns.eq(save.created_at_ns),
                        dsl::added_by_inbox_id.eq(&save.added_by_inbox_id),
                        dsl::message_disappear_from_ns.eq(save.message_disappear_from_ns),
                        dsl::message_disappear_in_ns.eq(save.message_disappear_in_ns),
                        dsl::should_publish_commit_log.eq(false),
                    ))
                    .execute(conn)
            })?;
            merge_activity(&db, &group_id, save.last_message_ns)?;
            StoredRestoredGroupMetadata {
                group_id,
                group_save: save.encode_to_vec(),
            }
            .store(&db)?;
            Ok(Continue(true))
        })
        .map(TransactionOutcome::into_continued)
    }
}

fn merge_activity(
    db: &impl DbQuery,
    group_id: &GroupId,
    timestamp: Option<i64>,
) -> Result<bool, StorageError> {
    let Some(timestamp) = timestamp else {
        return Ok(false);
    };
    use xmtp_db::diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;
    Ok(db.raw_query(|conn| {
        xmtp_db::diesel::update(dsl::groups.find(group_id))
            .filter(
                dsl::last_message_ns
                    .is_null()
                    .or(dsl::last_message_ns.lt(timestamp)),
            )
            .set(dsl::last_message_ns.eq(Some(timestamp)))
            .execute(conn)
    })? > 0)
}

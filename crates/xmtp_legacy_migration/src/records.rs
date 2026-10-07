//! Raw legacy SQL extraction. Current SDK models must not define these rows.
use crate::{MigrationError, MigrationReport, RecordError};
use diesel::{connection::DefaultLoadingMode, prelude::*, sql_types::*};
use prost::Message;
use xmtp_proto::xmtp::{
    device_sync::{
        BackupMetadataSave,
        backup_element::Element,
        consent_backup::{ConsentSave, ConsentStateSave},
        group_backup::GroupSave,
        message_backup::GroupMessageSave,
    },
    message_contents::EncodedContent,
};

const GROUP_PAGE: i64 = 64;

#[derive(QueryableByName)]
struct Group {
    #[diesel(sql_type = Binary)]
    id: Vec<u8>,
    #[diesel(sql_type = BigInt)]
    created_at_ns: i64,
    #[diesel(sql_type = Integer)]
    membership_state: i32,
    #[diesel(sql_type = BigInt)]
    installations_last_checked: i64,
    #[diesel(sql_type = Text)]
    added_by_inbox_id: String,
    #[diesel(sql_type = Nullable<BigInt>)]
    sequence_id: Option<i64>,
    #[diesel(sql_type = BigInt)]
    rotated_at_ns: i64,
    #[diesel(sql_type = Integer)]
    conversation_type: i32,
    #[diesel(sql_type = Nullable<Text>)]
    dm_id: Option<String>,
    #[diesel(sql_type = Nullable<BigInt>)]
    last_message_ns: Option<i64>,
    #[diesel(sql_type = Nullable<BigInt>)]
    message_disappear_from_ns: Option<i64>,
    #[diesel(sql_type = Nullable<BigInt>)]
    message_disappear_in_ns: Option<i64>,
    #[diesel(sql_type = Nullable<Text>)]
    paused_for_version: Option<String>,
}

#[derive(QueryableByName)]
struct MessageRow {
    #[diesel(sql_type = Binary)]
    id: Vec<u8>,
    #[diesel(sql_type = Binary)]
    group_id: Vec<u8>,
    #[diesel(sql_type = Binary)]
    decrypted_message_bytes: Vec<u8>,
    #[diesel(sql_type = BigInt)]
    sent_at_ns: i64,
    #[diesel(sql_type = Binary)]
    sender_installation_id: Vec<u8>,
    #[diesel(sql_type = Text)]
    sender_inbox_id: String,
    #[diesel(sql_type = Integer)]
    delivery_status: i32,
    #[diesel(sql_type = Integer)]
    content_type: i32,
    #[diesel(sql_type = Integer)]
    version_major: i32,
    #[diesel(sql_type = Integer)]
    version_minor: i32,
    #[diesel(sql_type = Text)]
    authority_id: String,
    #[diesel(sql_type = Nullable<Binary>)]
    reference_id: Option<Vec<u8>>,
    #[diesel(sql_type = BigInt)]
    originator_id: i64,
    #[diesel(sql_type = BigInt)]
    sequence_id: i64,
}

#[derive(QueryableByName)]
struct Consent {
    #[diesel(sql_type = Integer)]
    entity_type: i32,
    #[diesel(sql_type = Integer)]
    state: i32,
    #[diesel(sql_type = Text)]
    entity: String,
    #[diesel(sql_type = BigInt)]
    consented_at_ns: i64,
}

#[derive(QueryableByName)]
struct Blob {
    #[diesel(sql_type = Binary)]
    value_bytes: Vec<u8>,
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

fn context_blob(conn: &mut SqliteConnection, id: &[u8]) -> Result<Option<Blob>, MigrationError> {
    Ok(diesel::sql_query(
        "SELECT value_bytes FROM openmls_key_value WHERE key_bytes = ? AND version = 1 AND typeof(value_bytes) = 'blob' AND length(value_bytes) <= ?",
    )
    .bind::<Binary, _>(crate::metadata::context_key(id))
    .bind::<BigInt, _>(crate::metadata::MAX_CONTEXT_BYTES as i64)
    .get_result::<Blob>(conn)
    .optional()?)
}

fn invalid(reason: &'static str) -> MigrationError {
    MigrationError::RecordRead(RecordError::Invalid(reason))
}

// implements: MIG-001, MIG-006
pub(crate) fn export(
    conn: &mut SqliteConnection,
    archive_path: String,
    mut emit: impl FnMut(Element) -> Result<(), MigrationError>,
) -> Result<MigrationReport, MigrationError> {
    let mut report = MigrationReport {
        archive_path,
        group_count: 0,
        message_count: 0,
        consent_count: 0,
    };
    emit(Element::Metadata(BackupMetadataSave {
        elements: vec![1, 2],
        exported_at_ns: xmtp_common::time::now_ns(),
        start_ns: None,
        end_ns: None,
    }))?;
    let malformed = diesel::sql_query("SELECT count(*) AS count FROM groups WHERE conversation_type IS NULL OR membership_state IS NULL OR (conversation_type NOT IN (3,4) AND membership_state != 4 AND (typeof(id) != 'blob' OR length(id) != 16 OR conversation_type NOT IN (1,2) OR membership_state NOT IN (1,2,3,5)))").get_result::<Count>(conn)?;
    if malformed.count != 0 {
        return Err(invalid("invalid required group identity or enum"));
    }
    let mut after = Vec::new();
    loop {
        let page = diesel::sql_query("SELECT * FROM groups WHERE conversation_type IN (1, 2) AND membership_state IN (1, 2, 3, 5) AND id > ? ORDER BY id LIMIT ?")
            .bind::<Binary, _>(&after).bind::<BigInt, _>(GROUP_PAGE).load::<Group>(conn)?;
        if page.is_empty() {
            break;
        }
        for group in page {
            if group.id.len() != 16
                || !matches!(group.membership_state, 1..=5)
                || !matches!(group.conversation_type, 1 | 2)
            {
                return Err(invalid("invalid required group identity or enum"));
            }
            after = group.id.clone();
            let blob = context_blob(conn, &group.id)?;
            let (metadata, mutable_metadata) = blob
                .map(|blob| crate::metadata::decode(&blob.value_bytes))
                .unwrap_or_default();
            emit(Element::Group(GroupSave {
                id: group.id,
                created_at_ns: group.created_at_ns,
                membership_state: group.membership_state,
                installations_last_checked: group.installations_last_checked,
                added_by_inbox_id: group.added_by_inbox_id,
                welcome_id: group.sequence_id,
                rotated_at_ns: group.rotated_at_ns,
                conversation_type: group.conversation_type,
                dm_id: group.dm_id,
                last_message_ns: group.last_message_ns,
                message_disappear_from_ns: group.message_disappear_from_ns,
                message_disappear_in_ns: group.message_disappear_in_ns,
                metadata,
                mutable_metadata,
                paused_for_version: group.paused_for_version,
            }))?;
            report.group_count += 1;
        }
    }
    // Keep NULL expiry as unknown. Current group settings do not establish a
    // historical message expiry (MIG-006; ARCH-009 gap waiver).
    let unknown_kind = diesel::sql_query("SELECT count(*) AS count FROM group_messages m LEFT JOIN groups g ON m.group_id = g.id WHERE m.expire_at_ns IS NULL AND m.kind IS NULL AND (g.id IS NULL OR (g.conversation_type IN (1,2) AND g.membership_state IN (1,2,3,5)))")
        .get_result::<Count>(conn)?;
    if unknown_kind.count != 0 {
        return Err(invalid("missing required message kind"));
    }
    let orphan = diesel::sql_query("SELECT count(*) AS count FROM group_messages m LEFT JOIN groups g ON m.group_id = g.id WHERE g.id IS NULL AND m.kind = 1 AND m.expire_at_ns IS NULL")
            .get_result::<Count>(conn)?;
    if orphan.count != 0 {
        return Err(invalid("message has no required group"));
    }
    let messages = diesel::sql_query("SELECT m.* FROM group_messages m JOIN groups g ON m.group_id = g.id WHERE g.conversation_type IN (1, 2) AND g.membership_state IN (1, 2, 3, 5) AND m.kind = 1 AND m.expire_at_ns IS NULL ORDER BY m.id")
            .load_iter::<MessageRow, DefaultLoadingMode>(conn)?;
    for message in messages {
        emit(Element::GroupMessage(message?.into_save()?))?;
        report.message_count += 1;
    }
    for consent in diesel::sql_query("SELECT * FROM consent_records ORDER BY entity_type, entity")
        .load_iter::<Consent, DefaultLoadingMode>(conn)?
    {
        let consent = consent?;
        if !matches!(consent.entity_type, 1 | 2) || consent.entity.is_empty() {
            return Err(invalid("invalid required consent identity or enum"));
        }
        emit(Element::Consent(ConsentSave {
            entity_type: consent.entity_type,
            state: match consent.state {
                0 => ConsentStateSave::Unknown,
                1 => ConsentStateSave::Allowed,
                2 => ConsentStateSave::Denied,
                _ => return Err(invalid("invalid required consent state")),
            } as i32,
            entity: consent.entity,
            consented_at_ns: consent.consented_at_ns,
        }))?;
        report.consent_count += 1;
    }
    Ok(report)
}

impl MessageRow {
    #[allow(deprecated)]
    fn into_save(self) -> Result<GroupMessageSave, MigrationError> {
        if self.id.is_empty() || self.group_id.len() != 16 || !matches!(self.delivery_status, 1..=3)
        {
            return Err(invalid(
                "invalid required message identity or delivery status",
            ));
        }
        let mut save = GroupMessageSave {
            id: self.id,
            group_id: self.group_id,
            decrypted_message_bytes: self.decrypted_message_bytes,
            sent_at_ns: self.sent_at_ns,
            kind: 1,
            sender_installation_id: self.sender_installation_id,
            sender_inbox_id: self.sender_inbox_id,
            delivery_status: self.delivery_status,
            content_type_save: 0,
            version_major: self.version_major,
            version_minor: self.version_minor,
            authority_id: self.authority_id,
            reference_id: self.reference_id,
            sequence_id: Some(self.sequence_id),
            originator_id: Some(self.originator_id),
            content_type: legacy_content_type(self.content_type).to_owned(),
        };
        if let Ok(content) = EncodedContent::decode(save.decrypted_message_bytes.as_slice())
            && let Some(id) = content.r#type
        {
            save.authority_id = id.authority_id;
            save.content_type = id.type_id;
            save.version_major = id.version_major as i32;
            save.version_minor = id.version_minor as i32;
        }
        Ok(save)
    }
}

fn legacy_content_type(value: i32) -> &'static str {
    match value {
        1 => "text",
        2 => "group_membership_change",
        3 => "group_updated",
        4 => "reaction",
        5 => "readReceipt",
        6 => "reply",
        7 => "attachment",
        8 => "remoteStaticAttachment",
        9 => "transactionReference",
        10 => "walletSendCalls",
        11 => "leave_request",
        12 => "markdown",
        13 => "actions",
        14 => "intent",
        15 => "multiRemoteStaticAttachment",
        16 => "deleteMessage",
        _ => "unknown",
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    // verifies: MIG-005
    #[xmtp_common::test(unwrap_try = true)]
    fn oversized_context_is_not_loaded() {
        let mut conn = SqliteConnection::establish(":memory:")?;
        diesel::sql_query(
            "CREATE TABLE openmls_key_value (key_bytes BLOB, version INTEGER, value_bytes BLOB)",
        )
        .execute(&mut conn)?;
        let id = [1u8; 16];
        for size in [
            crate::metadata::MAX_CONTEXT_BYTES,
            crate::metadata::MAX_CONTEXT_BYTES + 1,
        ] {
            diesel::sql_query("DELETE FROM openmls_key_value").execute(&mut conn)?;
            diesel::sql_query("INSERT INTO openmls_key_value VALUES (?, 1, zeroblob(?))")
                .bind::<Binary, _>(crate::metadata::context_key(&id))
                .bind::<BigInt, _>(size as i64)
                .execute(&mut conn)?;
            let result = context_blob(&mut conn, &id)?;
            if size <= crate::metadata::MAX_CONTEXT_BYTES {
                assert_eq!(result.unwrap().value_bytes.len(), size);
            } else {
                assert!(
                    result.is_none(),
                    "oversized context reached the Rust row decoder"
                );
            }
        }
    }
}

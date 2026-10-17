use super::*;

// verifies: MIG-002, MIG-004, MIG-005
#[xmtp_common::test(unwrap_try = true)]
async fn malformed_deletion_fields_fail_without_publishing() {
    let mut failures = vec![];
    for (field, value) in [
        ("group_id", "CAST(group_id AS TEXT)"),
        ("deleted_message_id", "CAST(deleted_message_id AS TEXT)"),
        ("deleted_by_inbox_id", "CAST(deleted_by_inbox_id AS BLOB)"),
        ("group_id", "NULL"),
        ("deleted_message_id", "NULL"),
        ("deleted_by_inbox_id", "NULL"),
        ("is_super_admin_deletion", "2"),
        ("is_super_admin_deletion", "0.5"),
        ("is_super_admin_deletion", "'invalid'"),
        ("is_super_admin_deletion", "NULL"),
        ("is_super_admin_deletion", "4294967296"),
    ] {
        let (directory, args) = fixture("stable.db3");
        edit(
            &args,
            &format!(
                "INSERT INTO message_deletions(id, group_id, deleted_message_id, deleted_by_inbox_id, is_super_admin_deletion, deleted_at_ns) SELECT randomblob(32), group_id, id, sender_inbox_id, 0, sent_at_ns FROM group_messages WHERE id=x'{}'; CREATE TABLE deletion_copy AS SELECT * FROM message_deletions; DROP TABLE message_deletions; ALTER TABLE deletion_copy RENAME TO message_deletions; UPDATE message_deletions SET {field}={value};",
                "01".repeat(32)
            ),
        );
        failures.extend(
            rejects_without_publishing(
                &args,
                directory.path(),
                &format!("{field}={value}"),
                ExpectedError::RecordRead,
            )
            .await,
        );
    }
    for (field, value) in [
        ("id", "CAST(id AS TEXT)"),
        ("sender_inbox_id", "CAST(sender_inbox_id AS BLOB)"),
        ("group_id", "CAST(group_id AS TEXT)"),
    ] {
        let (directory, args) = fixture("stable.db3");
        edit(
            &args,
            &format!(
                "INSERT INTO message_deletions(id, group_id, deleted_message_id, deleted_by_inbox_id, is_super_admin_deletion, deleted_at_ns) SELECT randomblob(32), group_id, id, sender_inbox_id, 0, sent_at_ns FROM group_messages WHERE id=x'{}'; UPDATE group_messages SET {field}={value} WHERE id=x'{}';",
                "01".repeat(32),
                "01".repeat(32)
            ),
        );
        failures.extend(
            rejects_without_publishing(
                &args,
                directory.path(),
                &format!("message.{field}"),
                ExpectedError::RecordRead,
            )
            .await,
        );
    }
    assert!(
        failures.is_empty(),
        "malformed deletion fields were accepted: {failures:?}"
    );
}

// verifies: MIG-001, MIG-002, MIG-005
#[xmtp_common::test(unwrap_try = true)]
async fn canonical_deletions_keep_legacy_authority_and_type_rules() {
    for (author, flag, remaining) in [(OWNER, 0, 2), (PEER, 0, 3), (PEER, 1, 2)] {
        for (authority, major) in [("xmtp.org", 1), ("custom.example", 99)] {
            let (_directory, args) = fixture("stable.db3");
            edit(
                &args,
                &format!(
                    "INSERT INTO message_deletions(id, group_id, deleted_message_id, deleted_by_inbox_id, is_super_admin_deletion, deleted_at_ns) SELECT randomblob(32), group_id, id, '{author}', {flag}, sent_at_ns FROM group_messages WHERE id=x'{}';",
                    "01".repeat(32)
                ),
            );
            use prost::Message;
            let mut content = xmtp_proto::xmtp::message_contents::EncodedContent::decode(
                hex::decode(CONTENT_HEX)?.as_slice(),
            )?;
            let kind = content.r#type.as_mut().unwrap();
            kind.authority_id = authority.to_owned();
            kind.version_major = major;
            edit(
                &args,
                &format!(
                    "UPDATE group_messages SET decrypted_message_bytes=x'{}' WHERE id=x'{}'",
                    hex::encode(content.encode_to_vec()),
                    "01".repeat(32)
                ),
            );
            let before = source_bytes(&args);
            let report = prepare_migration_archive(args.clone()).await?;
            assert_eq!(report.message_count, remaining);
            assert_eq!(source_bytes(&args), before);
            let messages: Vec<_> = elements(&report.archive_path)
                .await
                .into_iter()
                .filter_map(|element| match element {
                    Element::GroupMessage(message) => Some(message),
                    _ => None,
                })
                .collect();
            assert_eq!(messages.len() as u64, remaining);
            assert_eq!(
                messages.iter().any(|message| message.id == vec![1; 32]),
                remaining == 3
            );
        }
    }
}

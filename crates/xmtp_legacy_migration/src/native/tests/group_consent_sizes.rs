use super::*;

const LIMIT: usize = 64 * 1024 * 1024;
const GROUP_ID: &str = "44444444444444444444444444444444";

fn sized_group(args: &PrepareMigrationArchiveArgs, fields: &str) {
    edit(
        args,
        &format!(
            "UPDATE groups SET added_by_inbox_id='', dm_id=NULL, paused_for_version=NULL \
             WHERE id=x'{GROUP_ID}'; UPDATE groups SET {fields} WHERE id=x'{GROUP_ID}';"
        ),
    );
}

// verifies: MIG-002, MIG-004, MIG-007
#[xmtp_common::test(unwrap_try = true)]
async fn oversized_group_rows_fail_without_publishing() {
    let mut failures = vec![];
    for (name, fields) in [
        (
            "added_by_inbox_id",
            format!("added_by_inbox_id=printf('%.*c', {}, 'a')", LIMIT + 1),
        ),
        ("dm_id", format!("dm_id=printf('%.*c', {}, 'd')", LIMIT + 1)),
        (
            "paused_for_version with embedded NULs",
            format!("paused_for_version=CAST(zeroblob({}) AS TEXT)", LIMIT + 1),
        ),
        (
            "combined group fields",
            format!(
                "added_by_inbox_id=printf('%.*c', {}, 'a'), dm_id=printf('%.*c', {}, 'd')",
                LIMIT / 2,
                LIMIT / 2
            ),
        ),
        // id 16 + inbox 4 + DM 1 + pause LIMIT - 20 = LIMIT + 1 bytes.
        (
            "group UTF-8 bytes",
            format!(
                "added_by_inbox_id='éé', dm_id='d', paused_for_version=printf('%.*c', {}, 'p')",
                LIMIT - 20
            ),
        ),
    ] {
        let (directory, args) = fixture("stable.db3");
        sized_group(&args, &fields);
        failures.extend(
            rejects_without_publishing(&args, directory.path(), name, ExpectedError::RecordRead)
                .await,
        );
    }
    assert!(
        failures.is_empty(),
        "oversized group rows accepted: {failures:?}"
    );
}

// verifies: MIG-002, MIG-004, MIG-007
#[xmtp_common::test(unwrap_try = true)]
async fn oversized_consent_rows_fail_without_publishing() {
    let mut failures = vec![];
    for (name, entity_type, entity) in [
        (
            "conversation consent",
            1,
            format!("printf('%.*c', {}, 'a')", LIMIT + 1),
        ),
        (
            "inbox consent",
            2,
            format!("printf('%.*c', {}, 'b')", LIMIT + 1),
        ),
        (
            "consent UTF-8 bytes",
            2,
            format!("printf('%.*c', {}, 'c') || 'é'", LIMIT - 1),
        ),
        (
            "consent embedded NULs",
            1,
            format!("CAST(zeroblob({}) AS TEXT)", LIMIT + 1),
        ),
    ] {
        let (directory, args) = fixture("stable.db3");
        edit(
            &args,
            &format!("UPDATE consent_records SET entity_type={entity_type}, entity={entity}"),
        );
        failures.extend(
            rejects_without_publishing(&args, directory.path(), name, ExpectedError::RecordRead)
                .await,
        );
    }
    assert!(
        failures.is_empty(),
        "oversized consent rows accepted: {failures:?}"
    );
}

// verifies: MIG-001, MIG-002, MIG-007
#[xmtp_common::test(unwrap_try = true)]
async fn exact_group_row_byte_limit_remains_supported() {
    let (directory, args) = fixture("stable.db3");
    // id 16 + inbox 2 + DM 1 + pause LIMIT - 19 = LIMIT bytes.
    sized_group(
        &args,
        &format!(
            "added_by_inbox_id='é', dm_id='d', paused_for_version=printf('%.*c', {}, 'p')",
            LIMIT - 19
        ),
    );
    let before = source_bytes(&args);
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (2, 3, 1)
    );
    let records = elements(&args.output_path).await;
    let group = records
        .iter()
        .find_map(|element| match element {
            Element::Group(group) if group.id == vec![0x44; 16] => Some(group),
            _ => None,
        })
        .unwrap();
    assert_eq!(group.added_by_inbox_id, "é");
    assert_eq!(group.dm_id.as_deref(), Some("d"));
    let pause = group.paused_for_version.as_ref().unwrap();
    assert_eq!(pause.len(), LIMIT - 19);
    assert!(pause.bytes().all(|byte| byte == b'p'));
    assert_eq!(source_bytes(&args), before);
    assert_eq!(entries(directory.path()), ["history.xmtp", "source.db3"]);
}

// verifies: MIG-001, MIG-002, MIG-007
#[xmtp_common::test(unwrap_try = true)]
async fn exact_consent_row_byte_limit_remains_supported() {
    for entity_type in [1, 2] {
        let (directory, args) = fixture("stable.db3");
        edit(
            &args,
            &format!(
                "UPDATE consent_records SET entity_type={entity_type}, entity=printf('%.*c', {LIMIT}, 'c')"
            ),
        );
        let before = source_bytes(&args);
        let report = prepare_migration_archive(args.clone()).await?;
        assert_eq!(
            (
                report.group_count,
                report.message_count,
                report.consent_count
            ),
            (2, 3, 1)
        );
        let records = elements(&args.output_path).await;
        let consent = records
            .iter()
            .find_map(|element| match element {
                Element::Consent(consent) => Some(consent),
                _ => None,
            })
            .unwrap();
        assert_eq!(consent.entity_type, entity_type);
        assert_eq!(consent.entity.len(), LIMIT);
        assert!(consent.entity.bytes().all(|byte| byte == b'c'));
        assert_eq!(consent.state, 2);
        assert_eq!(consent.consented_at_ns, 1700000000000000009);
        assert_eq!(source_bytes(&args), before);
        assert_eq!(entries(directory.path()), ["history.xmtp", "source.db3"]);
    }
}

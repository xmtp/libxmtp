use super::*;

const LIMIT: usize = 64 * 1024 * 1024;

fn sized_message(
    args: &PrepareMigrationArchiveArgs,
    content: usize,
    reference: usize,
    inbox: &str,
) {
    edit(
        args,
        &format!(
            "UPDATE group_messages SET decrypted_message_bytes=zeroblob({content}), \
             sender_installation_id=x'01', sender_inbox_id='{inbox}', \
             authority_id='a', reference_id=zeroblob({reference}) WHERE id=x'{}';",
            "01".repeat(32)
        ),
    );
}

fn entries(path: &Path) -> Vec<std::ffi::OsString> {
    let mut names = fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    names
}

// verifies: MIG-002, MIG-004, MIG-007
#[xmtp_common::test(unwrap_try = true)]
async fn oversized_message_rows_fail_without_publishing() {
    let mut failures = vec![];
    for (name, content, reference, inbox) in [
        ("single field", LIMIT + 1, 1, "é"),
        ("combined fields", LIMIT / 2, LIMIT / 2, "é"),
        // All fields total LIMIT + 1 bytes, but fewer than LIMIT characters.
        ("UTF-8 bytes", LIMIT - 54, 1, "éé"),
    ] {
        let (directory, args) = fixture("stable.db3");
        sized_message(&args, content, reference, inbox);
        let before = source_bytes(&args);
        for previous in [false, true] {
            if previous {
                fs::write(&args.output_path, b"completed archive")?;
            }
            let before_entries = entries(directory.path());
            let result = prepare_migration_archive(args.clone()).await;
            if !matches!(result, Err(MigrationError::RecordRead(_))) {
                failures.push(format!("{name}, previous={previous}: {result:?}"));
                if Path::new(&args.output_path).exists() {
                    fs::remove_file(&args.output_path)?;
                }
            } else {
                if previous {
                    assert_eq!(fs::read(&args.output_path)?, b"completed archive");
                } else {
                    assert!(!Path::new(&args.output_path).exists());
                }
                assert_eq!(
                    entries(directory.path()),
                    before_entries,
                    "private output remains"
                );
            }
            assert_eq!(source_bytes(&args), before);
        }
    }
    assert!(
        failures.is_empty(),
        "oversized rows were accepted: {failures:?}"
    );
}

// verifies: MIG-001, MIG-002, MIG-007
#[xmtp_common::test(unwrap_try = true)]
async fn exact_message_row_byte_limit_remains_supported() {
    let (directory, args) = fixture("stable.db3");
    // id 32 + group 16 + installation 1 + inbox 2 + authority 1 + reference 1.
    let content_size = LIMIT - 53;
    sized_message(&args, content_size, 1, "é");
    let before = source_bytes(&args);
    let mut expected_entries = entries(directory.path());
    expected_entries.push("history.xmtp".into());
    expected_entries.sort();
    let report = prepare_migration_archive(args.clone()).await?;
    assert_eq!(
        (
            report.group_count,
            report.message_count,
            report.consent_count
        ),
        (2, 3, 1)
    );
    let restored = elements(&args.output_path).await;
    let message = restored
        .iter()
        .find_map(|element| match element {
            Element::GroupMessage(message) if message.id == vec![1; 32] => Some(message),
            _ => None,
        })
        .unwrap();
    assert_eq!(message.decrypted_message_bytes.len(), content_size);
    assert!(
        message
            .decrypted_message_bytes
            .iter()
            .all(|byte| *byte == 0)
    );
    assert_eq!(message.sender_installation_id, vec![1]);
    assert_eq!(message.sender_inbox_id, "é");
    assert_eq!(message.authority_id, "a");
    assert_eq!(message.reference_id.as_deref(), Some([0].as_slice()));
    assert_eq!(source_bytes(&args), before);
    assert_eq!(entries(directory.path()), expected_entries);
}

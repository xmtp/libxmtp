use crate::tester;

// verifies: GMOD-037
#[xmtp_common::test(unwrap_try = true)]
async fn test_group_id_is_sixteen_bytes_and_not_derived() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    // Two groups from one client, and one from another, so a derived id would
    // repeat for the same creator or share bytes with the creator's identity.
    let first = alix.create_group(None, None)?;
    let second = alix.create_group(None, None)?;
    let other = bo.create_group(None, None)?;

    let ids = [&first.group_id, &second.group_id, &other.group_id];
    for id in ids {
        assert_eq!(id.len(), 16, "a group id is 16 bytes");
    }

    assert_ne!(
        first.group_id, second.group_id,
        "one creator's groups must not share an id"
    );
    assert_ne!(
        first.group_id, other.group_id,
        "two creators' groups must not share an id"
    );

    // An id built from the creator's identity would contain those bytes.
    for id in ids {
        assert!(
            !contains(id.as_slice(), alix.inbox_id().as_bytes()),
            "a group id must not carry the inbox id"
        );
        assert!(
            !contains(id.as_slice(), alix.context.installation_id().as_slice()),
            "a group id must not carry the installation key"
        );
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

use super::*;
use crate::test_utils::with_connection;

#[xmtp_common::test(unwrap_try = true)]
fn a_fresh_database_holds_no_copy() {
    with_connection(|conn| {
        assert_eq!(conn.server_configuration().unwrap(), None);
    });
}

#[xmtp_common::test(unwrap_try = true)]
fn storing_twice_replaces_the_one_row() {
    with_connection(|conn| {
        conn.store_server_configuration("org.example.one", "http://a:5050", b"first", 1)
            .unwrap();
        let stored = conn.server_configuration().unwrap().unwrap();
        assert_eq!(stored.identifier, "org.example.one");
        assert_eq!(stored.backend_url, "http://a:5050");
        assert_eq!(stored.response, b"first".to_vec());
        assert_eq!(stored.fetched_at_ns, 1);
        assert_eq!(stored.conflicting_identifier, None);

        // A URL change that keeps the identifier rewrites the same row.
        conn.store_server_configuration("org.example.one", "http://b:5050", b"second", 2)
            .unwrap();
        let stored = conn.server_configuration().unwrap().unwrap();
        assert_eq!(stored.backend_url, "http://b:5050");
        assert_eq!(stored.response, b"second".to_vec());
        assert_eq!(stored.fetched_at_ns, 2);
        assert_eq!(stored.id, 0);
    });
}

// verifies: CONF-031
#[xmtp_common::test(unwrap_try = true)]
fn a_refresh_write_never_clears_a_recorded_conflict() {
    with_connection(|conn| {
        conn.store_server_configuration("org.example.one", "http://a:5050", b"first", 1)
            .unwrap();
        conn.record_server_configuration_conflict("org.example.two")
            .unwrap();
        assert_eq!(
            conn.server_configuration()
                .unwrap()
                .unwrap()
                .conflicting_identifier,
            Some("org.example.two".to_owned())
        );

        // A matching refresh rewrites the copy and leaves the conflict in place.
        conn.store_server_configuration("org.example.one", "http://a:5050", b"third", 3)
            .unwrap();
        let stored = conn.server_configuration().unwrap().unwrap();
        assert_eq!(stored.response, b"third".to_vec());
        assert_eq!(
            stored.conflicting_identifier,
            Some("org.example.two".to_owned())
        );
    });
}

#[xmtp_common::test(unwrap_try = true)]
fn recording_a_conflict_with_no_stored_copy_is_a_no_op() {
    with_connection(|conn| {
        conn.record_server_configuration_conflict("org.example.two")
            .unwrap();
        assert_eq!(conn.server_configuration().unwrap(), None);
    });
}

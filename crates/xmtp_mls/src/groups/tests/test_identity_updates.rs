//! The engine picks the inboxes whose identity updates it must load.

use crate::builder::ClientBuilder;
use xmtp_common::rand_vec;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::{
    ConnectionExt, db_connection::DbConnection, identity_update::StoredIdentityUpdate, prelude::*,
};

fn insert_identity_update<C>(conn: &DbConnection<C>, inbox_id: &str, sequence_id: i64)
where
    C: ConnectionExt,
{
    let identity_update =
        StoredIdentityUpdate::new(inbox_id.to_string(), sequence_id, 0, rand_vec::<24>());

    conn.insert_or_ignore_identity_updates(&[identity_update])
        .expect("insert should succeed");
}

#[rstest::rstest]
#[xmtp_common::test]
async fn load_identity_updates_if_needed() {
    let wallet = generate_local_wallet();
    let client = ClientBuilder::new_test_client(&wallet).await;
    let conn = client.context.db();

    insert_identity_update(&conn, "inbox_1", 1);
    insert_identity_update(&conn, "inbox_2", 2);
    insert_identity_update(&conn, "inbox_3", 3);

    let filtered =
        // Inbox 1 is requesting an inbox ID higher than what is in the DB. Inbox 2 is requesting one that matches the DB.
        // Inbox 3 is requesting one lower than what is in the DB
        crate::groups::filter_inbox_ids_needing_updates(&conn, &[("inbox_1", 3), ("inbox_2", 2), ("inbox_3", 2)]);
    assert_eq!(filtered.unwrap(), vec!["inbox_1"]);
}

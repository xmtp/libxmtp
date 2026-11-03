//! Every state write that prepares a publication commits durably.

use crate::context::CoreContext;
use crate::core::precommit_test_hook;
use crate::tester;

#[cfg(not(target_arch = "wasm32"))]
// verifies: STORE-026, SEND-001, SEND-006
#[rstest::rstest]
#[case::queue("queue")]
#[case::attempt("attempt")]
#[case::welcomes("welcomes")]
#[case::key_package("key_package")]
#[xmtp_common::test(unwrap_try = true)]
async fn publication_prepare_points_are_durable(
    #[case] site: &'static str,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::groups::mls_sync::publish::PreparedAttempt;
    use std::{cell::Cell, rc::Rc};
    use xmtp_db::{
        ConnectionExt,
        diesel::{self, prelude::*},
        schema::group_intents,
    };
    #[derive(QueryableByName)]
    struct Scalar {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        value: i32,
    }
    tester!(alix, persistent_db, disable_workers);
    tester!(bo, persistent_db, disable_workers);
    let group = alix.create_group(None, None)?;
    let count = match site {
        "key_package" => "SELECT COUNT(*) AS value FROM key_package_history",
        _ => "SELECT COUNT(*) AS value FROM group_messages",
    };
    let scalar = |conn: &mut diesel::SqliteConnection, sql: &str| {
        let Scalar { value } = diesel::sql_query(sql).get_result(conn).unwrap();
        value
    };
    let db = alix.context.db();
    let before = db.raw_query(|conn| Ok(scalar(conn, count)))?;
    let observed = Rc::new(Cell::new(None));
    let record = observed.clone();
    let _hook = precommit_test_hook::install(move |conn| {
        if record.get().is_some() {
            return;
        }
        let appeared = match site {
            "queue" | "key_package" => scalar(conn, count) > before,
            _ => group_intents::table
                .select(group_intents::prepared_envelopes)
                .load::<Option<Vec<u8>>>(conn)
                .unwrap()
                .into_iter()
                .flatten()
                .any(|bytes| {
                    site == "attempt" || PreparedAttempt::decode(&bytes).unwrap().welcomes.is_some()
                }),
        };
        if appeared {
            record.set(Some(scalar(
                conn,
                "SELECT synchronous AS value FROM pragma_synchronous",
            )));
        }
    });
    match site {
        "queue" => {
            group.send_message_optimistic(b"durable", Default::default())?;
        }
        "attempt" => {
            group.send_message(b"durable", Default::default()).await?;
        }
        "welcomes" => {
            group.add_members(&[bo.inbox_id()]).await?;
        }
        _ => {
            crate::identity_lifecycle::generate_and_store_key_package(
                alix.context.identity(),
                alix.context.mls_storage(),
                false,
            )?;
        }
    }
    assert_eq!(observed.get(), Some(2), "{site}");
    Ok(())
}

// See src/lib.rs: the concrete async client future exceeds rustc's default
// layout query depth; an integration test is its own crate root, so repeat it.
#![recursion_limit = "512"]
//! End-to-end MLS group flows for the async client, over a live XMTP node and
//! Postgres. These are the traffic-carrying tests the async storage work was
//! missing: registration, welcome processing, key-package consumption, the
//! commit-log key and message crypto all run through the sqlx-backed
//! `XmtpMlsStorageProvider` KV, and every assertion below is a row that had to
//! survive a Postgres round-trip.
//!
//! Skipped unless `XMTP_ASYNCDB_PG_URL` is set; see the crate docs for how to run.

use std::time::Duration;

use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::groups::{MlsGroup, send_message_opts::SendMessageOpts};
use xmtp_mls_pg_tests::{pg_url_or_skip, register_client};
use xmtp_proto::types::GroupId;

/// Sync the group and look for an application message whose bytes equal
/// `expected`, retrying to absorb network propagation delay. Panics with the
/// group's decrypted history if it never arrives.
async fn wait_for_message<C>(group: &MlsGroup<C>, expected: &[u8])
where
    C: XmtpSharedContext,
{
    for _ in 0..20 {
        group.sync().await.expect("sync");
        let msgs = group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::Application),
                ..Default::default()
            })
            .await
            .expect("find_messages");
        if msgs.iter().any(|m| m.decrypted_message_bytes == expected) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let msgs = group
        .find_messages(&MsgQueryArgs {
            kind: Some(GroupMessageKind::Application),
            ..Default::default()
        })
        .await
        .expect("find_messages");
    let seen: Vec<String> = msgs
        .iter()
        .map(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).into_owned())
        .collect();
    panic!(
        "expected message {:?} never arrived; group saw application messages: {seen:?}",
        String::from_utf8_lossy(expected)
    );
}

/// Find the group with `group_id` among a client's welcomed groups.
fn welcomed<'a, C>(groups: &'a [MlsGroup<C>], group_id: &GroupId) -> &'a MlsGroup<C> {
    groups
        .iter()
        .find(|g| g.group_id == *group_id)
        .expect("the created group should appear in sync_welcomes")
}

/// Two fresh identities on the async/Postgres store create a shared group and
/// exchange messages in both directions. Exercises welcome creation (alix) and
/// welcome processing (bo), then a reply from the welcomed member back to the
/// creator — the fullest single-flow test of the async storage path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_clients_exchange_messages_over_postgres() {
    let url = pg_url_or_skip!();
    xmtp_mls_pg_tests::init_logging();

    let alix = register_client(&url, "alix").await.expect("register alix");
    let bo = register_client(&url, "bo").await.expect("register bo");
    eprintln!(
        "REGISTERED alix={} bo={}",
        &alix.inbox_id[..12],
        &bo.inbox_id[..12]
    );

    // alix creates the group and adds bo, then sends the first message.
    let alix_group = alix
        .client
        .create_group(None, None)
        .await
        .expect("create_group");
    alix_group
        .add_members(&[bo.inbox_id.as_str()])
        .await
        .expect("add bo");
    alix_group
        .send_message(b"hello from alix", SendMessageOpts::default())
        .await
        .expect("alix send");

    // bo pulls the welcome, finds the group, and reads alix's message.
    let bo_groups = bo.client.sync_welcomes().await.expect("bo sync_welcomes");
    let bo_group = welcomed(&bo_groups, &alix_group.group_id);
    wait_for_message(bo_group, b"hello from alix").await;

    // bo replies; alix reads it back — the return direction.
    bo_group
        .send_message(b"hi from bo", SendMessageOpts::default())
        .await
        .expect("bo send");
    wait_for_message(&alix_group, b"hi from bo").await;

    // Both identities and their group state persisted to their own Postgres
    // schemas (not each other's, not SQLite).
    assert!(alix.count("identity").await >= 1, "alix identity persisted");
    assert!(bo.count("identity").await >= 1, "bo identity persisted");
    assert!(alix.count("groups").await >= 1, "alix group persisted");
    assert!(bo.count("groups").await >= 1, "bo group persisted");

    // PROTOTYPE: libxmtp's own KV data now lands in purpose-built tables; there is
    // no generic `openmls_key_value` at all (kv_routing.rs asserts its absence).
    eprintln!(
        "TYPED KV TABLES (bo): kp_references={} kp_wrapper_private_keys={} commit_log_signer_keys={}",
        bo.count("kp_references").await,
        bo.count("kp_wrapper_private_keys").await,
        bo.count("commit_log_signer_keys").await,
    );
    assert!(
        bo.count("kp_references").await >= 1,
        "key-package references now persist to the typed kp_references table"
    );

    eprintln!("ROUND TRIP OK: bidirectional messaging over Postgres, typed KV tables");
}

/// A three-member group: two members are welcomed concurrently off a single
/// membership commit, and both must independently process the welcome and read
/// the creator's message. Covers key-package consumption for more than one
/// joiner in one epoch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_member_group_over_postgres() {
    let url = pg_url_or_skip!();
    xmtp_mls_pg_tests::init_logging();

    let alix = register_client(&url, "alix3").await.expect("register alix");
    let bo = register_client(&url, "bo3").await.expect("register bo");
    let caro = register_client(&url, "caro3").await.expect("register caro");

    let group = alix
        .client
        .create_group(None, None)
        .await
        .expect("create_group");
    group
        .add_members(&[bo.inbox_id.as_str(), caro.inbox_id.as_str()])
        .await
        .expect("add bo + caro");
    group
        .send_message(b"welcome all", SendMessageOpts::default())
        .await
        .expect("alix send");

    for (who, client) in [("bo", &bo), ("caro", &caro)] {
        let groups = client
            .client
            .sync_welcomes()
            .await
            .unwrap_or_else(|e| panic!("{who} sync_welcomes: {e}"));
        let joined = welcomed(&groups, &group.group_id);
        wait_for_message(joined, b"welcome all").await;
    }

    // Everyone sees the same three-member roster.
    let members = group.members().await.expect("members");
    assert_eq!(members.len(), 3, "group should have three members");

    eprintln!("THREE MEMBER OK: two concurrent welcomes processed over Postgres");
}

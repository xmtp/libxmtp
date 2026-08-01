//! The sqlx `QueryIcebox` impl.
//!
//! The icebox holds envelopes whose parents have not been seen yet, plus the
//! `(envelope, dependency)` edges between them, and the two `WITH RECURSIVE`
//! walks traverse that graph in both directions. These tests build a known
//! three-link chain and then break it in each of the two ways a cursor can be
//! wrong — wrong originator, wrong sequence — because a recursive join that
//! matched on only half the cursor would still return a plausible-looking chain.

use xmtp_db::group::{GroupMembershipState, QueryGroup, StoredGroup};
use xmtp_db::icebox::QueryIcebox;
use xmtp_db::pg::PgDb;
use xmtp_db::refresh_state::{EntityKind, QueryRefreshState};
use xmtp_db_pg_tests::fresh_db;
use xmtp_proto::types::{Cursor, GroupId, OrphanedEnvelope};

fn gid(n: u8) -> GroupId {
    GroupId::from([n; 16])
}

async fn make_group(db: &PgDb, id: GroupId) {
    db.insert_or_replace_group(
        StoredGroup::builder()
            .id(id)
            .created_at_ns(0)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("inbox")
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
}

/// `Cursor::new(sequence_id, originator_id)` — note the order.
fn orphan(group_id: GroupId, cursor: Cursor, depends_on: Cursor) -> OrphanedEnvelope {
    OrphanedEnvelope::builder()
        .cursor(cursor)
        .depending_on(depends_on)
        .payload(vec![1, 2, 3])
        .group_id(group_id)
        .build()
        .unwrap()
}

/// The chain the sync tests use: (41,1) -> (40,1) -> (39,2) -> (38,2), where
/// (38,2) itself is not iceboxed.
fn chain(group_id: GroupId) -> Vec<OrphanedEnvelope> {
    vec![
        orphan(group_id, Cursor::new(41, 1u32), Cursor::new(40, 1u32)),
        orphan(group_id, Cursor::new(40, 1u32), Cursor::new(39, 2u32)),
        orphan(group_id, Cursor::new(39, 2u32), Cursor::new(38, 2u32)),
    ]
}

fn sequences(envelopes: &[OrphanedEnvelope]) -> Vec<u64> {
    let mut sequences: Vec<u64> = envelopes.iter().map(|e| e.cursor.sequence_id).collect();
    sequences.sort();
    sequences
}

async fn iced_chain(name: &str) -> PgDb {
    let db = fresh_db(name).await;
    make_group(&db, gid(1)).await;
    db.ice(chain(gid(1))).await.unwrap();
    db
}

// --- ice --------------------------------------------------------------------

#[tokio::test]
async fn ice_counts_envelopes_and_their_dependency_edges() {
    let db = fresh_db("ice_count").await;
    make_group(&db, gid(1)).await;

    // 3 envelopes + 1 dependency edge each.
    assert_eq!(db.ice(chain(gid(1))).await.unwrap(), 6);
    // Re-icing the same envelopes conflicts on every row.
    assert_eq!(db.ice(chain(gid(1))).await.unwrap(), 0);
    assert_eq!(db.ice(vec![]).await.unwrap(), 0);
}

/// The bulk insert has to survive duplicates *within* one batch, not only
/// against rows already stored — `ON CONFLICT DO NOTHING` covers both, where
/// `DO UPDATE` would error.
#[tokio::test]
async fn ice_tolerates_duplicates_inside_one_batch() {
    let db = fresh_db("ice_dupes").await;
    make_group(&db, gid(1)).await;

    let duplicated = vec![
        orphan(gid(1), Cursor::new(41, 1u32), Cursor::new(40, 1u32)),
        orphan(gid(1), Cursor::new(41, 1u32), Cursor::new(40, 1u32)),
    ];
    assert_eq!(
        db.ice(duplicated).await.unwrap(),
        2,
        "one envelope and one edge"
    );
}

/// An envelope with several unmet parents keeps one edge per parent, and they
/// all come back on the envelope's `depends_on`.
#[tokio::test]
async fn ice_stores_every_dependency_of_an_envelope() {
    let db = fresh_db("ice_multi_dep").await;
    make_group(&db, gid(1)).await;

    let multi = OrphanedEnvelope::builder()
        .cursor(Cursor::new(50, 1u32))
        .depending_on(Cursor::new(49, 1u32))
        .depending_on(Cursor::new(20, 7u32))
        .payload(vec![9])
        .group_id(gid(1))
        .build()
        .unwrap();
    assert_eq!(
        db.ice(vec![multi]).await.unwrap(),
        3,
        "1 envelope + 2 edges"
    );

    let found = db
        .future_dependents(&[Cursor::new(49, 1u32)])
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].depends_on[&1], 49);
    assert_eq!(found[0].depends_on[&7], 20);
    assert_eq!(found[0].payload.as_ref(), &[9]);
}

// --- past_dependents --------------------------------------------------------

#[tokio::test]
async fn past_dependents_walks_the_whole_chain() {
    let db = iced_chain("ice_past").await;
    let found = db.past_dependents(&[Cursor::new(41, 1u32)]).await.unwrap();
    assert_eq!(sequences(&found), vec![39, 40, 41]);

    // Starting from the middle only reaches what is below it.
    let found = db.past_dependents(&[Cursor::new(40, 1u32)]).await.unwrap();
    assert_eq!(sequences(&found), vec![39, 40]);
}

/// The second base case: a cursor that is *not* itself iceboxed still returns
/// the chain hanging off its immediate dependencies.
#[tokio::test]
async fn past_dependents_starts_from_dependencies_when_the_cursor_is_not_iceboxed() {
    let db = fresh_db("ice_past_gap").await;
    make_group(&db, gid(1)).await;
    db.ice(vec![orphan(
        gid(1),
        Cursor::new(40, 1u32),
        Cursor::new(39, 2u32),
    )])
    .await
    .unwrap();

    // (41,1) was never iceboxed, but it depends on (40,1), which is.
    db.ice(vec![orphan(
        gid(1),
        Cursor::new(41, 1u32),
        Cursor::new(40, 1u32),
    )])
    .await
    .unwrap();

    let found = db.past_dependents(&[Cursor::new(41, 1u32)]).await.unwrap();
    assert_eq!(sequences(&found), vec![40, 41]);
}

#[tokio::test]
async fn past_dependents_of_nothing_is_empty() {
    let db = iced_chain("ice_past_empty").await;
    assert!(db.past_dependents(&[]).await.unwrap().is_empty());
    assert!(
        db.past_dependents(&[Cursor::new(999, 9u32)])
            .await
            .unwrap()
            .is_empty()
    );
}

// --- future_dependents ------------------------------------------------------

#[tokio::test]
async fn future_dependents_walks_up_the_chain_without_the_cursor_itself() {
    let db = iced_chain("ice_future").await;
    let found = db
        .future_dependents(&[Cursor::new(39, 2u32)])
        .await
        .unwrap();
    assert_eq!(
        sequences(&found),
        vec![40, 41],
        "everything blocked behind (39,2), but not (39,2)"
    );

    let mut found = db
        .future_dependents(&[Cursor::new(38, 2u32)])
        .await
        .unwrap();
    found.sort_by_key(|e| e.cursor.sequence_id);
    assert_eq!(sequences(&found), vec![39, 40, 41]);
    assert_eq!(found[0].depends_on[&2], 38);
    assert_eq!(found[1].depends_on[&2], 39);
    assert_eq!(found[2].depends_on[&1], 40);
}

// --- a cursor is a pair, and both halves must match -------------------------

/// (40,1) depends on (39,**2**); re-pointing the third envelope at originator 1
/// breaks the chain, and a join matching only on `sequence_id` would not notice.
#[tokio::test]
async fn a_wrong_originator_breaks_the_chain() {
    let db = fresh_db("ice_wrong_orig").await;
    make_group(&db, gid(1)).await;
    let mut orphans = chain(gid(1));
    orphans[2] = orphan(gid(1), Cursor::new(39, 1u32), Cursor::new(38, 1u32));
    db.ice(orphans).await.unwrap();

    let found = db.past_dependents(&[Cursor::new(41, 1u32)]).await.unwrap();
    assert_eq!(sequences(&found), vec![40, 41], "(39,1) is not (39,2)");

    assert!(
        db.future_dependents(&[Cursor::new(39, 1u32)])
            .await
            .unwrap()
            .is_empty()
    );
}

/// The same test in the other axis: right originator, wrong sequence.
#[tokio::test]
async fn a_wrong_sequence_breaks_the_chain() {
    let db = fresh_db("ice_wrong_seq").await;
    make_group(&db, gid(1)).await;
    let mut orphans = chain(gid(1));
    orphans[2] = orphan(gid(1), Cursor::new(100, 2u32), Cursor::new(38, 2u32));
    db.ice(orphans).await.unwrap();

    let found = db.past_dependents(&[Cursor::new(41, 1u32)]).await.unwrap();
    assert_eq!(sequences(&found), vec![40, 41]);
}

// --- prune ------------------------------------------------------------------

/// An icebox entry is dead once the group's refresh cursor for that originator
/// has reached it — the envelope was processed some other way.
#[tokio::test]
async fn prune_drops_entries_the_cursor_has_passed() {
    let db = iced_chain("ice_prune").await;
    assert_eq!(db.prune_icebox().await.unwrap(), 0, "no cursors yet");

    // Originator 1 has been processed through sequence 40, so (40,1) goes and
    // (41,1) stays. Originator 2 is untouched, so (39,2) stays.
    db.update_cursor(
        gid(1),
        EntityKind::ApplicationMessage,
        Cursor::new(40, 1u32),
    )
    .await
    .unwrap();
    assert_eq!(db.prune_icebox().await.unwrap(), 1);

    let left = db
        .future_dependents(&[Cursor::new(38, 2u32)])
        .await
        .unwrap();
    assert_eq!(
        sequences(&left),
        vec![39],
        "(40,1) is gone, so (41,1) is unreachable"
    );
}

/// Only the two message kinds count: a commit-log cursor at the same sequence
/// must not prune an application message.
#[tokio::test]
async fn prune_ignores_unrelated_entity_kinds() {
    let db = iced_chain("ice_prune_kind").await;
    db.update_cursor(
        gid(1),
        EntityKind::CommitLogDownload,
        Cursor::new(999, 1u32),
    )
    .await
    .unwrap();
    assert_eq!(db.prune_icebox().await.unwrap(), 0);

    db.update_cursor(gid(1), EntityKind::CommitMessage, Cursor::new(999, 1u32))
        .await
        .unwrap();
    assert_eq!(
        db.prune_icebox().await.unwrap(),
        2,
        "both originator-1 rows"
    );
}

/// The cursor is scoped to the group, so another group's progress prunes
/// nothing.
#[tokio::test]
async fn prune_is_scoped_to_the_group() {
    let db = iced_chain("ice_prune_group").await;
    make_group(&db, gid(2)).await;
    db.update_cursor(
        gid(2),
        EntityKind::ApplicationMessage,
        Cursor::new(999, 1u32),
    )
    .await
    .unwrap();
    assert_eq!(db.prune_icebox().await.unwrap(), 0);
}

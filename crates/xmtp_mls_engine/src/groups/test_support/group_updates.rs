//! Group-update rows that startup and duplicate-check tests seed.

use prost::Message;
use xmtp_common::{rand_vec, time::now_ns};
use xmtp_content_types::{ContentCodec, encryption::sha256, group_updated::GroupUpdatedCodec};
use xmtp_db::diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_db::{ConnectionExt, DbConnection, Store};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::mls::message_contents::GroupUpdated;
use xmtp_proto::xmtp::mls::message_contents::group_updated::Inbox;

/// The number of group updates one page of the update check reads.
pub const PAGE_SIZE: i64 = crate::groups::mls_sync::RECEIVE_PAGE_SIZE;

/// Marks the rows a test seeds, so they can be pinned to one `inserted_at_ns`.
pub(crate) const SEEDED_SENDER: &str = "seeded";

/// A group update that adds `inbox_id`.
pub fn add_update(inbox_id: &str) -> GroupUpdated {
    GroupUpdated {
        added_inboxes: vec![Inbox {
            inbox_id: inbox_id.to_string(),
        }],
        ..Default::default()
    }
}

/// The encoded content of `update`, as a stored group update holds it.
pub fn encode(update: &GroupUpdated) -> Vec<u8> {
    let mut bytes = Vec::new();
    GroupUpdatedCodec::encode(update.clone())
        .unwrap()
        .encode(&mut bytes)
        .unwrap();
    bytes
}

/// Store `rows` as group updates of `group_id`, in order, then pin them to
/// one `inserted_at_ns` one millisecond above every row the group already
/// holds.
pub fn seed_tied_updates<C: ConnectionExt>(
    db: &DbConnection<C>,
    group_id: GroupId,
    rows: Vec<Vec<u8>>,
) {
    for (i, bytes) in rows.into_iter().enumerate() {
        StoredGroupMessage {
            id: sha256(&rand_vec::<12>()),
            group_id,
            decrypted_message_bytes: bytes,
            sent_at_ns: now_ns(),
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id: vec![1, 2, 3],
            sender_inbox_id: SEEDED_SENDER.to_string(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::GroupUpdated,
            version_major: 0,
            version_minor: 0,
            authority_id: "unknown".to_string(),
            reference_id: None,
            sequence_id: i as i64 + 1,
            envelope_hash: None,
            expiry_ns: None,
            expiry_unknown: false,
            deleted_by_inbox_id: None,
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push: true,
            idempotency_key: String::new(),
        }
        .store(db)
        .unwrap();
    }

    // The database stamps `inserted_at_ns` at insert time, and its clock
    // does not agree with `now_ns`. Pin the seeded rows to one value just
    // above every row the group already holds, rather than rely on how
    // fast the loop ran or on the wall clock.
    db.raw_query(|conn| {
        use xmtp_db::schema::group_messages::dsl;
        let newest: Option<i64> = dsl::group_messages
            .filter(dsl::group_id.eq(group_id))
            .select(xmtp_db::diesel::dsl::max(dsl::inserted_at_ns))
            .first(conn)?;
        let shared_inserted_at_ns = newest.unwrap_or_default() + 1_000_000;
        xmtp_db::diesel::update(dsl::group_messages)
            .filter(dsl::group_id.eq(group_id))
            .filter(dsl::sender_inbox_id.eq(SEEDED_SENDER))
            .set(dsl::inserted_at_ns.eq(shared_inserted_at_ns))
            .execute(conn)
    })
    .unwrap();
}

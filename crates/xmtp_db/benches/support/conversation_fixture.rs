use diesel::connection::SimpleConnection;
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::fmt::Write;
use xmtp_common::{NS_IN_30_DAYS, NS_IN_DAY, NS_IN_HOUR, NS_IN_SEC, time::now_ns};
use xmtp_content_types::{
    ContentCodec, membership_change::GroupMembershipChangeCodec, text::TextCodec,
};
use xmtp_db::consent_record::{ConsentState, ConsentType};
use xmtp_db::group::{ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
use xmtp_db::{ConnectionExt, EncryptedMessageStore, NativeDb};

pub const SEED: u64 = 4278;
pub const PAGE_SIZE: i64 = 50;
const SYNC_GROUPS: usize = 5;
const FIXTURE_AGE_NS: i64 = 9 * NS_IN_DAY;
const PAYLOAD_BYTES: usize = 256;
const SEED_BATCH: usize = 100;

pub fn open(path: &str) -> EncryptedMessageStore<NativeDb> {
    let db = NativeDb::builder()
        .persistent(path)
        .key([0u8; 32])
        .single_connection()
        .build()
        .expect("build benchmark database");
    EncryptedMessageStore::new(db).expect("open benchmark database")
}

/// Insert a fixed data set without a network client. The SQL uses base tables.
/// Expired rows stay in the database so the list must check their expiry.
/// Each run starts the data set nine days ago.
/// This keeps both live and expired rows when the benchmark runs at a later date.
pub fn seed(store: &EncryptedMessageStore<NativeDb>, count: usize) {
    let created_ns = now_ns() - FIXTURE_AGE_NS;
    assert!(count > SYNC_GROUPS);
    let mut rng = StdRng::seed_from_u64(SEED);
    let payload = "ab".repeat(PAYLOAD_BYTES);
    let text_type = TextCodec::content_type();
    let membership_type = GroupMembershipChangeCodec::content_type();
    store
        .db()
        .raw_query(|conn| {
            conn.batch_execute("BEGIN IMMEDIATE;")?;
            let mut sql = String::new();
            for i in 0..count {
                let id = format!("{i:032x}");
                let created = created_ns + i as i64 * NS_IN_SEC;
                let is_sync = i < SYNC_GROUPS;
                let is_dm = !is_sync && i % 10 == 0;
                let kind = if is_sync {
                    ConversationType::Sync
                } else if is_dm {
                    ConversationType::Dm
                } else {
                    ConversationType::Group
                };
                let dm = if is_dm {
                    format!("'own:peer-{i}'")
                } else {
                    "NULL".to_owned()
                };
                let disappearing = !is_sync && rng.random_range(0..4) == 0;
                let short_expiry = disappearing && rng.random_range(0..2) == 0;
                let disappear_in = if short_expiry { NS_IN_HOUR } else { NS_IN_30_DAYS };
                let (from, duration) = if disappearing {
                    (created.to_string(), disappear_in.to_string())
                } else {
                    ("NULL".to_owned(), "NULL".to_owned())
                };
                writeln!(sql, "INSERT INTO groups(id,created_at_ns,membership_state,installations_last_checked,added_by_inbox_id,conversation_type,dm_id,message_disappear_from_ns,message_disappear_in_ns) VALUES(X'{id}',{created},{membership},0,'bench-inbox',{kind},{dm},{from},{duration});", membership = GroupMembershipState::Allowed as i32, kind = kind as i32).expect("format group");
                // 60% Allowed, 30% Unknown (half absent), 10% Denied.
                let consent = rng.random_range(0..20);
                let state = match consent {
                    0..12 => Some(ConsentState::Allowed),
                    12..15 => Some(ConsentState::Unknown),
                    15..18 => None,
                    _ => Some(ConsentState::Denied),
                };
                if let Some(state) = state {
                    writeln!(sql, "INSERT INTO consent_records(entity_type,state,entity,consented_at_ns) VALUES({entity_type},{state},'{id}',0);", entity_type = ConsentType::ConversationId as i32, state = state as i32).expect("format consent");
                }
                // 20% empty; 45% small; 25% medium; 10% busy.
                let messages = match rng.random_range(0..100) {
                    0..20 => 0,
                    20..65 => rng.random_range(1..=4),
                    65..90 => rng.random_range(5..=15),
                    _ => rng.random_range(16..=50),
                };

                for j in 0..messages {
                    let message_id = format!("{:032x}{j:032x}", i + 1);
                    let sent = created + (j + 1) as i64 * NS_IN_SEC;
                    let (content_type, version) = if j % 7 == 3 {
                        (ContentType::GroupMembershipChange, &membership_type)
                    } else {
                        (ContentType::Text, &text_type)
                    };
                    let expiry = if disappearing && j >= messages / 2 {
                        (disappear_in.to_string(), (sent + disappear_in).to_string())
                    } else {
                        ("NULL".to_owned(), "NULL".to_owned())
                    };
                    writeln!(sql, "INSERT INTO group_messages(id,group_id,decrypted_message_bytes,sent_at_ns,kind,sender_installation_id,sender_inbox_id,delivery_status,content_type,version_major,version_minor,authority_id,sequence_id,inserted_at_ns,expiry_ns,expire_at_ns) VALUES(X'{message_id}',X'{id}',X'{payload}',{sent},{kind},X'{id}','bench-sender',{delivery_status},{content_type},{version_major},{version_minor},'{authority_id}',{}, {sent},{},{});",j+1,expiry.0,expiry.1, kind = GroupMessageKind::Application as i32, delivery_status = DeliveryStatus::Published as i32, content_type = content_type as i32, version_major = version.version_major, version_minor = version.version_minor, authority_id = version.authority_id).expect("format message");
                }
                if i % SEED_BATCH == SEED_BATCH - 1 {
                    conn.batch_execute(&sql)?;
                    sql.clear();
                }
            }
            conn.batch_execute(&sql)?;
            conn.batch_execute("COMMIT; PRAGMA wal_checkpoint(TRUNCATE);")
        })
        .expect("seed benchmark database");
}

/// Common app filters. The default order is creation time.
pub fn cases() -> Vec<(&'static str, GroupQueryArgs)> {
    let page = GroupQueryArgs {
        limit: Some(PAGE_SIZE),
        ..Default::default()
    };
    vec![
        ("default_page", page.clone()),
        ("default_full", GroupQueryArgs::default()),
        (
            "allowed_page",
            GroupQueryArgs {
                consent_states: Some(vec![ConsentState::Allowed]),
                ..page.clone()
            },
        ),
        (
            "allowed_unknown_page",
            GroupQueryArgs {
                consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
                ..page.clone()
            },
        ),
        (
            "groups_page",
            GroupQueryArgs {
                conversation_type: Some(ConversationType::Group),
                ..page.clone()
            },
        ),
        (
            "dms_page",
            GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..page.clone()
            },
        ),
        (
            "sync_page",
            GroupQueryArgs {
                include_sync_groups: true,
                ..page.clone()
            },
        ),
        (
            "activity_page",
            GroupQueryArgs {
                order_by: Some(GroupQueryOrderBy::LastActivity),
                ..page.clone()
            },
        ),
        (
            "activity_full",
            GroupQueryArgs {
                order_by: Some(GroupQueryOrderBy::LastActivity),
                ..Default::default()
            },
        ),
    ]
}

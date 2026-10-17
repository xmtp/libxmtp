use super::ConnectionExt;
use crate::consent_record::ConsentState;
use crate::group::{ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
use crate::group_message::{ContentType, DeliveryStatus, GroupMessageKind};
use crate::{DbConnection, StorageError};
use diesel::query_builder::{BoxedSqlQuery, SqlQuery};
use diesel::sql_types::{BigInt, Integer};
use diesel::sqlite::Sqlite;
use diesel::{QueryableByName, RunQueryDsl, sql_query};
use serde::{Deserialize, Serialize};

/// Content types eligible for the latest conversation message.
pub const CONVERSATION_LIST_CONTENT_TYPES: &[ContentType] = &[
    ContentType::Unknown,
    ContentType::Text,
    ContentType::Reaction,
    ContentType::Reply,
    ContentType::Attachment,
    ContentType::RemoteAttachment,
    ContentType::TransactionReference,
    ContentType::WalletSendCalls,
];

#[derive(QueryableByName, Debug, Clone, Deserialize, Serialize)]
/// A group and its latest app-visible message, when one exists.
pub struct ConversationListItem {
    /// Group ID.
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub id: xmtp_proto::types::GroupId,
    /// Time of the Welcome.
    #[diesel(sql_type = BigInt)]
    pub created_at_ns: i64,
    /// Current membership state.
    #[diesel(sql_type = Integer)]
    pub membership_state: GroupMembershipState,
    /// Last installation check.
    #[diesel(sql_type = BigInt)]
    pub installations_last_checked: i64,
    /// Inbox that added this installation.
    #[diesel(sql_type = diesel::sql_types::Text)]
    pub added_by_inbox_id: String,
    /// Welcome sequence ID.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub welcome_sequence_id: Option<i64>,
    /// Canonical DM identity, if this is a DM.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub dm_id: Option<String>,
    /// Last leaf-key rotation.
    #[diesel(sql_type = BigInt)]
    pub rotated_at_ns: i64,
    /// Conversation kind.
    #[diesel(sql_type = Integer)]
    pub conversation_type: ConversationType,
    /// Whether the remote commit log is forked.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Bool>)]
    pub is_commit_log_forked: Option<bool>,
    /// Message ID, absent when no eligible live message exists.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Binary>)]
    pub message_id: Option<Vec<u8>>,
    /// Decrypted message content.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Binary>)]
    pub decrypted_message_bytes: Option<Vec<u8>>,
    /// Message send time.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub sent_at_ns: Option<i64>,
    /// Message kind.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub kind: Option<GroupMessageKind>,
    /// Sender installation.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Binary>)]
    pub sender_installation_id: Option<Vec<u8>>,
    /// Sender inbox.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub sender_inbox_id: Option<String>,
    /// Delivery state.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub delivery_status: Option<DeliveryStatus>,
    /// Content type.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub content_type: Option<ContentType>,
    /// Content type major version.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub version_major: Option<i32>,
    /// Content type minor version.
    #[diesel(sql_type = diesel::sql_types::Nullable<Integer>)]
    pub version_minor: Option<i32>,
    /// Content type authority.
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    pub authority_id: Option<String>,
    /// Message sequence ID.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub sequence_id: Option<i64>,
    /// Backend retention deadline, separate from disappearing-message expiry.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub expiry_ns: Option<i64>,
    /// Disappearing-message deadline.
    #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
    pub expire_at_ns: Option<i64>,
}

/// Group columns of a list row. A list query selects a page of groups first
/// and only then reads one latest message for each group on that page, so a
/// page never ranks every stored message.
const PAGE_COLUMNS: &str = "g.id, g.created_at_ns, g.membership_state,
    g.installations_last_checked, g.added_by_inbox_id,
    g.sequence_id AS welcome_sequence_id, g.dm_id, g.rotated_at_ns,
    g.conversation_type, g.is_commit_log_forked";

/// A scalar subquery for one column of the latest message that the app can see
/// in `{group}`: an application message of a listed content type whose
/// disappearing deadline is after the bound time. The expiry predicate stays
/// inside the lookup, so an expired message never hides an older live one.
/// Equal send times go to the greater message id. One seek of
/// `group_messages_sent_at_id_sort` answers it without a table read.
///
/// Binds: the current time.
fn latest_live_message(column: &str, group: &str) -> String {
    let content_types = CONVERSATION_LIST_CONTENT_TYPES
        .iter()
        .map(|value| (*value as i32).to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "(SELECT gm.{column} FROM group_messages gm
          WHERE gm.group_id = {group}.id
            AND gm.kind = {application_kind}
            AND gm.content_type IN ({content_types})
            AND (gm.expire_at_ns IS NULL OR gm.expire_at_ns > ?)
          ORDER BY gm.sent_at_ns DESC, gm.id DESC
          LIMIT 1)",
        application_kind = GroupMessageKind::Application as i32,
    )
}

/// Close the `page` CTE and join each page row to its latest live message.
/// The rows keep the page order: `list_order_ns`, then the greater group id.
///
/// Binds: the current time.
fn with_latest_message(query: ListQuery, current_time_ns: i64) -> ListQuery {
    query
        .sql(format!(
            ") SELECT p.id, p.created_at_ns, p.membership_state,
                   p.installations_last_checked, p.added_by_inbox_id,
                   p.welcome_sequence_id, p.dm_id, p.rotated_at_ns,
                   p.conversation_type, p.is_commit_log_forked,
                   m.id AS message_id, m.decrypted_message_bytes, m.sent_at_ns,
                   m.kind, m.sender_installation_id, m.sender_inbox_id,
                   m.delivery_status, m.content_type, m.version_major,
                   m.version_minor, m.authority_id, m.sequence_id,
                   m.expiry_ns, m.expire_at_ns
            FROM page p
            LEFT JOIN group_messages m ON m.rowid = {}
            ORDER BY p.list_order_ns DESC, p.id DESC",
            latest_live_message("rowid", "p")
        ))
        .bind::<BigInt, _>(current_time_ns)
}

fn enum_values<T>(values: &[T], to_i32: impl Fn(&T) -> i32) -> String {
    values
        .iter()
        .map(|value| to_i32(value).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

type ListQuery = BoxedSqlQuery<'static, Sqlite, SqlQuery>;

/// Leading SQL that asks SQLite for the plan of a statement.
#[cfg(all(test, not(target_arch = "wasm32")))]
const EXPLAIN_QUERY_PLAN: &str = "EXPLAIN QUERY PLAN ";

/// The regular conversations that match `args`, one page at most.
fn regular_list_query(args: &GroupQueryArgs, current_time_ns: i64) -> ListQuery {
    build_regular_list_query("", args, current_time_ns)
}

/// The plan of `regular_list_query`.
#[cfg(all(test, not(target_arch = "wasm32")))]
fn explain_regular_list_query(args: &GroupQueryArgs, current_time_ns: i64) -> ListQuery {
    build_regular_list_query(EXPLAIN_QUERY_PLAN, args, current_time_ns)
}

/// Every sync group with its latest live message, newest group first.
fn sync_list_query(current_time_ns: i64) -> ListQuery {
    build_sync_list_query("", current_time_ns)
}

/// The plan of `sync_list_query`.
#[cfg(all(test, not(target_arch = "wasm32")))]
fn explain_sync_list_query(current_time_ns: i64) -> ListQuery {
    build_sync_list_query(EXPLAIN_QUERY_PLAN, current_time_ns)
}

/// `head` is SQL placed before the statement.
fn build_regular_list_query(head: &str, args: &GroupQueryArgs, current_time_ns: i64) -> ListQuery {
    let effective_consent_states = args
        .consent_states
        .clone()
        .unwrap_or_else(|| vec![ConsentState::Allowed, ConsentState::Unknown]);
    let includes_unknown = effective_consent_states.contains(&ConsentState::Unknown);
    let includes_all = effective_consent_states.len() == 3;
    let filtered_states: Vec<_> = effective_consent_states
        .iter()
        .filter(|state| **state != ConsentState::Unknown)
        .copied()
        .collect();
    // Activity is the send time of the latest live message, or the creation
    // time of a group without one. Only activity order and activity filters
    // read it, so a creation-order page seeks messages for its own rows only.
    // Binds: the current time.
    let activity = format!(
        "COALESCE({}, g.created_at_ns)",
        latest_live_message("sent_at_ns", "g")
    );

    let mut query =
        sql_query(format!("{head}WITH page AS (SELECT {PAGE_COLUMNS}, ")).into_boxed::<Sqlite>();
    query = match args.order_by.clone().unwrap_or_default() {
        GroupQueryOrderBy::CreatedAt => query.sql("g.created_at_ns"),
        GroupQueryOrderBy::LastActivity => query.sql(&activity).bind::<BigInt, _>(current_time_ns),
    };
    query = query.sql(" AS list_order_ns FROM groups g");

    if !includes_all {
        query = query.sql(
            " LEFT JOIN consent_records consent
              ON consent.entity = lower(hex(g.id))",
        );
    }
    query = query.sql(format!(
        " WHERE g.conversation_type NOT IN ({}, {})",
        ConversationType::Sync as i32,
        ConversationType::Oneshot as i32
    ));

    if !args.include_duplicate_dms {
        query = query.sql(format!(
            " AND NOT EXISTS (
                SELECT 1 FROM groups g2
                WHERE COALESCE(g2.dm_id, g2.id) = COALESCE(g.dm_id, g.id)
                AND (g2.membership_state != {restored}, COALESCE(g2.last_message_ns, 0), g2.id)
                  > (g.membership_state != {restored}, COALESCE(g.last_message_ns, 0), g.id)
            )",
            restored = GroupMembershipState::Restored as i32,
        ));
    }

    if let Some(states) = &args.allowed_states {
        if states.is_empty() {
            query = query.sql(" AND 0");
        } else {
            query = query.sql(format!(
                " AND g.membership_state IN ({})",
                enum_values(states, |state| *state as i32)
            ));
        }
    }
    if let Some(after) = args.last_activity_after_ns {
        query = query
            .sql(format!(" AND {activity} > ?"))
            .bind::<BigInt, _>(current_time_ns)
            .bind::<BigInt, _>(after);
    }
    if let Some(after) = args.created_after_ns {
        query = query
            .sql(" AND g.created_at_ns > ?")
            .bind::<BigInt, _>(after);
    }
    if let Some(before) = args.last_activity_before_ns {
        query = query
            .sql(format!(" AND {activity} < ?"))
            .bind::<BigInt, _>(current_time_ns)
            .bind::<BigInt, _>(before);
    }
    if let Some(before) = args.created_before_ns {
        query = query
            .sql(" AND g.created_at_ns < ?")
            .bind::<BigInt, _>(before);
    }
    if let Some(conversation_type) = args.conversation_type {
        query = query
            .sql(" AND g.conversation_type = ?")
            .bind::<Integer, _>(conversation_type as i32);
    }

    if !includes_all {
        if includes_unknown {
            query = query.sql(" AND (consent.state IS NULL OR consent.state = 0");
            if !filtered_states.is_empty() {
                query = query.sql(format!(
                    " OR consent.state IN ({})",
                    enum_values(&filtered_states, |state| *state as i32)
                ));
            }
            query = query.sql(")");
        } else {
            query = query.sql(format!(
                " AND consent.state IN ({})",
                enum_values(&filtered_states, |state| *state as i32)
            ));
        }
    }

    query = query.sql(" ORDER BY list_order_ns DESC, g.id DESC");
    if let Some(limit) = args.limit {
        query = query.sql(" LIMIT ?").bind::<BigInt, _>(limit);
    }
    with_latest_message(query, current_time_ns)
}

/// `head` is SQL placed before the statement.
fn build_sync_list_query(head: &str, current_time_ns: i64) -> ListQuery {
    let query = sql_query(format!(
        "{head}WITH page AS (SELECT {PAGE_COLUMNS}, g.created_at_ns AS list_order_ns
         FROM groups g WHERE g.conversation_type = ?"
    ))
    .into_boxed::<Sqlite>()
    .bind::<Integer, _>(ConversationType::Sync as i32);
    with_latest_message(query, current_time_ns)
}

pub trait QueryConversationList {
    fn fetch_conversation_list<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<ConversationListItem>, StorageError>;
}

impl<T> QueryConversationList for &T
where
    T: QueryConversationList,
{
    fn fetch_conversation_list<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        (**self).fetch_conversation_list(args)
    }
}

impl<C: ConnectionExt> QueryConversationList for DbConnection<C> {
    fn fetch_conversation_list<A: AsRef<GroupQueryArgs>>(
        &self,
        args: A,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        self.fetch_conversation_list_at(args.as_ref(), xmtp_common::time::now_ns())
    }
}

impl<C: ConnectionExt> DbConnection<C> {
    // implements: CONS-030
    // implements: META-051
    fn fetch_conversation_list_at(
        &self,
        args: &GroupQueryArgs,
        current_time_ns: i64,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        args.validate()?;
        if matches!(&args.consent_states, Some(states) if states.is_empty()) {
            return self.requested_sync_groups(args, current_time_ns);
        }

        let query = regular_list_query(args, current_time_ns);
        let mut conversations = self.raw_query(|conn| query.load::<ConversationListItem>(conn))?;

        conversations.append(&mut self.requested_sync_groups(args, current_time_ns)?);
        Ok(conversations)
    }

    /// Sync groups the app asked for, or none. They bypass the regular filters
    /// and limit, as before. A sync group is the user's own state, not a
    /// conversation, so a consent record, and the consent filter, has no effect
    /// on it (docs/specs/SYNC-device-sync.md, section 1).
    // implements: SYNC-005
    fn requested_sync_groups(
        &self,
        args: &GroupQueryArgs,
        current_time_ns: i64,
    ) -> Result<Vec<ConversationListItem>, StorageError> {
        if !args.requests_sync_groups() {
            return Ok(Vec::new());
        }
        let sync = sync_list_query(current_time_ns);
        Ok(self.raw_query(|conn| sync.load::<ConversationListItem>(conn))?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::Store;
    use crate::consent_record::{ConsentState, ConsentType};
    use crate::group::tests::{
        generate_consent_record, generate_dm, generate_group, generate_group_with_created_at,
    };
    use crate::group::{ConversationType, GroupMembershipState, GroupQueryArgs, GroupQueryOrderBy};
    use crate::group_message::ContentType;
    use crate::group_message::tests::generate_message;
    use crate::prelude::*;
    use crate::test_utils::with_connection;

    /// A page reads one latest message for each listed group through the
    /// message index. It never scans or sorts every stored message. The plan
    /// text belongs to the native SQLite build.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    fn list_plans_seek_the_latest_message_for_each_group() {
        use crate::ConnectionExt;
        use diesel::RunQueryDsl;

        #[derive(diesel::QueryableByName)]
        struct PlanStep {
            #[diesel(sql_type = diesel::sql_types::Text)]
            detail: String,
        }

        with_connection(|conn| -> Result<(), crate::StorageError> {
            let query_plan = |query: super::ListQuery| -> Result<Vec<String>, crate::StorageError> {
                let steps = conn.raw_query(|conn| query.load::<PlanStep>(conn))?;
                Ok(steps.into_iter().map(|step| step.detail).collect())
            };
            let mut plans = Vec::new();
            for order_by in [
                GroupQueryOrderBy::CreatedAt,
                GroupQueryOrderBy::LastActivity,
            ] {
                let args = GroupQueryArgs {
                    limit: Some(50),
                    order_by: Some(order_by),
                    ..Default::default()
                };
                plans.push(query_plan(super::explain_regular_list_query(&args, 1))?);
            }
            plans.push(query_plan(super::explain_sync_list_query(1))?);
            for plan in plans {
                assert!(
                    plan.iter()
                        .all(|step| !step.starts_with("SCAN gm") && !step.starts_with("SCAN m")),
                    "a list query scans group_messages: {plan:#?}"
                );
                assert!(
                    plan.iter().any(|step| step
                        == "SEARCH gm USING COVERING INDEX group_messages_sent_at_id_sort (group_id=?)"),
                    "a list query does not seek the latest message index: {plan:#?}"
                );
            }
            Ok(())
        })?
    }

    // verifies: META-051
    #[xmtp_common::test(unwrap_try = true)]
    fn latest_conversation_preview_uses_live_rows_before_ranking() {
        let now = 1_000_000;
        with_connection(|conn| -> Result<(), crate::StorageError> {
            let mixed = generate_group_with_created_at(None, 100);
            let persistent = generate_group_with_created_at(None, 150);
            let empty = generate_group_with_created_at(None, 175);
            let all_expired = generate_group_with_created_at(None, 250);
            for group in [&mixed, &persistent, &empty, &all_expired] {
                group.store(conn)?;
            }

            let mut older_live = generate_message(
                None,
                Some(&mixed.id),
                Some(300),
                Some(ContentType::Text),
                Some(now + 1),
                None,
            );
            older_live.expiry_ns = Some(now + 100);
            older_live.store(conn)?;
            let newer_expired = generate_message(
                None,
                Some(&mixed.id),
                Some(500),
                Some(ContentType::Text),
                Some(now - 1),
                None,
            );
            newer_expired.store(conn)?;
            let equality_expired = generate_message(
                None,
                Some(&all_expired.id),
                Some(450),
                Some(ContentType::Text),
                Some(now),
                None,
            );
            equality_expired.store(conn)?;
            let no_deadline = generate_message(
                None,
                Some(&persistent.id),
                Some(350),
                Some(ContentType::Text),
                None,
                None,
            );
            no_deadline.store(conn)?;

            assert!(conn.get_group_message(&newer_expired.id)?.is_some());
            assert!(conn.get_group_message(&equality_expired.id)?.is_some());
            let args = GroupQueryArgs {
                order_by: Some(GroupQueryOrderBy::LastActivity),
                ..Default::default()
            };
            let listed = conn.fetch_conversation_list_at(&args, now)?;
            assert_eq!(listed.len(), 4);
            assert_eq!(listed[0].id, persistent.id);
            assert_eq!(
                listed[0].message_id.as_deref(),
                Some(no_deadline.id.as_slice())
            );
            assert_eq!(listed[1].id, mixed.id);
            assert_eq!(
                listed[1].message_id.as_deref(),
                Some(older_live.id.as_slice())
            );
            assert_eq!(listed[1].expire_at_ns, older_live.expire_at_ns);
            assert_eq!(listed[1].expiry_ns, older_live.expiry_ns);
            assert_eq!(listed[2].id, all_expired.id);
            assert!(listed[2].message_id.is_none());
            assert_eq!(listed[3].id, empty.id);
            assert!(listed[3].message_id.is_none());

            let after = conn.fetch_conversation_list_at(
                &GroupQueryArgs {
                    last_activity_after_ns: Some(325),
                    ..args.clone()
                },
                now,
            )?;
            assert_eq!(after.len(), 1);
            assert_eq!(after[0].id, persistent.id);

            let bounded = conn.fetch_conversation_list_at(
                &GroupQueryArgs {
                    limit: Some(2),
                    ..args
                },
                now,
            )?;
            assert_eq!(bounded.len(), 2);
            assert_eq!(bounded[0].id, persistent.id);
            assert_eq!(bounded[1].id, mixed.id);
            Ok(())
        })?;
    }

    /// The latest message skips membership changes, excluded content types,
    /// and expired rows, and equal send times go to the greater message id.
    /// Regular conversations and requested sync groups follow the same rule.
    // verifies: META-051
    #[xmtp_common::test(unwrap_try = true)]
    fn latest_message_skips_ineligible_rows_and_prefers_the_greater_id() {
        use crate::group_message::GroupMessageKind;
        let now = 1_000_000;
        with_connection(|conn| -> Result<(), crate::StorageError> {
            let group = generate_group_with_created_at(None, 100);
            let mut sync_group = generate_group_with_created_at(None, 100);
            sync_group.conversation_type = ConversationType::Sync;
            // Message ids are unique across groups.
            for (group, base) in [(&group, 0u8), (&sync_group, 10)] {
                group.store(conn)?;
                // The greater id is stored first, so insertion order cannot
                // pick it.
                for id in [base + 2, base + 1] {
                    let mut tie = generate_message(
                        None,
                        Some(&group.id),
                        Some(500),
                        Some(ContentType::Text),
                        None,
                        None,
                    );
                    tie.id = vec![id; 32];
                    tie.store(conn)?;
                }
                for (kind, sent_at_ns, content_type, expire_at_ns) in [
                    (
                        GroupMessageKind::MembershipChange,
                        600,
                        ContentType::Text,
                        None,
                    ),
                    (
                        GroupMessageKind::Application,
                        700,
                        ContentType::ReadReceipt,
                        None,
                    ),
                    (
                        GroupMessageKind::Application,
                        800,
                        ContentType::Text,
                        Some(now),
                    ),
                ] {
                    generate_message(
                        Some(kind),
                        Some(&group.id),
                        Some(sent_at_ns),
                        Some(content_type),
                        expire_at_ns,
                        None,
                    )
                    .store(conn)?;
                }
            }

            for order_by in [
                GroupQueryOrderBy::CreatedAt,
                GroupQueryOrderBy::LastActivity,
            ] {
                let listed = conn.fetch_conversation_list_at(
                    &GroupQueryArgs {
                        include_sync_groups: true,
                        order_by: Some(order_by),
                        ..Default::default()
                    },
                    now,
                )?;
                let previews: Vec<_> = listed
                    .into_iter()
                    .map(|item| (item.id, item.message_id, item.sent_at_ns))
                    .collect();
                assert_eq!(
                    previews,
                    [
                        (group.id, Some(vec![2; 32]), Some(500)),
                        (sync_group.id, Some(vec![12; 32]), Some(500)),
                    ]
                );
            }
            Ok(())
        })?
    }

    /// Groups with the same order time follow the greater group id. The page
    /// limit and the returned order use the same rule, so a page is the start
    /// of the full list.
    #[xmtp_common::test(unwrap_try = true)]
    fn equal_order_times_follow_the_greater_group_id() {
        use xmtp_proto::types::GroupId;
        with_connection(|conn| -> Result<(), crate::StorageError> {
            let mut ids = Vec::new();
            // Stored out of id order, so storage order cannot pass the test.
            for id in [3u8, 1, 2] {
                let mut group = generate_group_with_created_at(None, 1_000);
                group.id = GroupId::from([id; 16]);
                group.store(conn)?;
                ids.push(group.id);
            }
            let [three, one, two] = ids[..] else {
                unreachable!()
            };

            for order_by in [
                GroupQueryOrderBy::CreatedAt,
                GroupQueryOrderBy::LastActivity,
            ] {
                let list = |limit| {
                    conn.fetch_conversation_list(GroupQueryArgs {
                        limit,
                        order_by: Some(order_by.clone()),
                        ..Default::default()
                    })
                    .map(|items| items.into_iter().map(|item| item.id).collect::<Vec<_>>())
                };
                assert_eq!(list(None)?, [three, two, one]);
                assert_eq!(list(Some(2))?, [three, two]);
            }
            Ok(())
        })?
    }

    #[xmtp_common::test]
    fn test_single_group_multiple_messages() {
        with_connection(|conn| {
            // Create a group
            let group = generate_group(None);
            group.store(conn).unwrap();

            // Insert multiple messages into the group
            for i in 1..5 {
                let message = crate::encrypted_store::group_message::tests::generate_message(
                    None,
                    Some(&group.id),
                    Some(i * 1000),
                    Some(ContentType::Text),
                    None,
                    None,
                );

                message.store(conn).unwrap();
            }

            // Fetch the conversation list
            let conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(conversation_list.len(), 1, "Should return one group");
            assert_eq!(
                conversation_list[0].id, group.id,
                "Returned group ID should match the created group"
            );
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                4000,
                "Last message should be the most recent one"
            );
        })
    }

    #[xmtp_common::test]
    fn test_three_groups_specific_ordering() {
        with_connection(|conn| {
            // Create three groups
            let group_a = generate_group_with_created_at(None, 5000); // Created after last message
            let group_b = generate_group_with_created_at(None, 2000); // Created before last message
            let group_c = generate_group_with_created_at(None, 1000); // Created before last message with no messages

            group_a.store(conn).unwrap();
            group_b.store(conn).unwrap();
            group_c.store(conn).unwrap();
            // Add a message to group_b
            let message = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group_b.id),
                Some(3000), // Last message timestamp
                None,
                None,
                None,
            );
            message.store(conn).unwrap();

            // Fetch the conversation list
            let conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();

            assert_eq!(conversation_list.len(), 3, "Should return all three groups");
            assert_eq!(
                conversation_list[0].id, group_a.id,
                "Group created after the last message should come first"
            );
            assert_eq!(
                conversation_list[1].id, group_b.id,
                "Group with the last message should come second"
            );
            assert_eq!(
                conversation_list[2].id, group_c.id,
                "Group created before the last message with no messages should come last"
            );
        })
    }

    #[xmtp_common::test]
    fn test_group_with_newer_message_update() {
        with_connection(|conn| {
            // Create a group
            let group = generate_group(None);
            group.store(conn).unwrap();

            // Add an initial message
            let first_message = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group.id),
                Some(1000),
                Some(ContentType::Text),
                None,
                None,
            );
            first_message.store(conn).unwrap();

            // Fetch the conversation list and check last message
            let mut conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(conversation_list.len(), 1, "Should return one group");
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                1000,
                "Last message should match the first message"
            );

            // Add a newer message
            let second_message = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group.id),
                Some(2000),
                Some(ContentType::Text),
                None,
                None,
            );
            second_message.store(conn).unwrap();

            // Fetch the conversation list again and validate the last message is updated
            conversation_list = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(
                conversation_list[0].sent_at_ns.unwrap(),
                2000,
                "Last message should now match the second (newest) message"
            );
        })
    }

    // verifies: CONS-030
    #[xmtp_common::test]
    fn test_find_conversations_by_consent_state() {
        with_connection(|conn| {
            let test_group_1 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_1.store(conn).unwrap();
            let test_group_2 = generate_group(Some(GroupMembershipState::Allowed));
            test_group_2.store(conn).unwrap();
            let test_group_3 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_3.store(conn).unwrap();
            let test_group_4 = generate_dm(Some(GroupMembershipState::Allowed));
            test_group_4.store(conn).unwrap();

            let test_group_1_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_1.id),
            );
            test_group_1_consent.store(conn).unwrap();
            let test_group_2_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(test_group_2.id),
            );
            test_group_2_consent.store(conn).unwrap();
            let test_group_3_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(test_group_3.id),
            );
            test_group_3_consent.store(conn).unwrap();

            let all_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![
                        ConsentState::Allowed,
                        ConsentState::Unknown,
                        ConsentState::Denied,
                    ]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(all_results.len(), 4);

            let default_results = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            assert_eq!(default_results.len(), 3);

            let allowed_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(allowed_results.len(), 2);

            let allowed_unknown_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(allowed_unknown_results.len(), 3);

            let denied_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Denied]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(denied_results.len(), 1);
            assert_eq!(denied_results[0].id, test_group_2.id);

            let unknown_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![ConsentState::Unknown]),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(unknown_results.len(), 1);
            assert_eq!(unknown_results[0].id, test_group_4.id);

            let empty_array_results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    ..Default::default()
                })
                .unwrap();
            assert!(empty_array_results.is_empty());

            let mut sync_group = generate_group(Some(GroupMembershipState::Allowed));
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn).unwrap();
            let with_sync = conn
                .fetch_conversation_list(GroupQueryArgs {
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            assert!(with_sync.iter().any(|group| group.id == sync_group.id));
            let empty_with_sync = conn
                .fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(vec![]),
                    include_sync_groups: true,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                empty_with_sync
                    .iter()
                    .map(|group| &group.id)
                    .collect::<Vec<_>>(),
                [&sync_group.id]
            );
        })
    }

    /// A consent record has no effect on a sync group. When the app asks for
    /// sync groups, a Denied sync group is listed for any consent filter,
    /// including an empty one.
    // verifies: SYNC-005
    #[xmtp_common::test(unwrap_try = true)]
    fn requested_sync_groups_ignore_the_consent_filter() {
        with_connection(|conn| -> Result<(), crate::StorageError> {
            let mut sync_group = generate_group(Some(GroupMembershipState::Allowed));
            sync_group.conversation_type = ConversationType::Sync;
            sync_group.store(conn)?;
            generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(sync_group.id.as_slice()),
            )
            .store(conn)?;

            for consent_states in [vec![ConsentState::Allowed], vec![]] {
                let listed = conn.fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(consent_states.clone()),
                    include_sync_groups: true,
                    ..Default::default()
                })?;
                assert!(
                    listed.iter().any(|group| group.id == sync_group.id),
                    "a Denied sync group was hidden by {consent_states:?}"
                );
                let without_sync = conn.fetch_conversation_list(GroupQueryArgs {
                    consent_states: Some(consent_states),
                    ..Default::default()
                })?;
                assert!(!without_sync.iter().any(|group| group.id == sync_group.id));
            }
            Ok(())
        })?
    }

    /// A `Restored` archive placeholder for a DM must not hide the row the
    /// client can act in, even when the placeholder holds the newer message
    /// and the higher id.
    #[xmtp_common::test]
    fn test_dm_list_prefers_joined_over_restored() {
        use crate::group::StoredGroup;
        use xmtp_common::{Generate, time::now_ns};
        use xmtp_proto::types::GroupId;

        with_connection(|conn| {
            let dm_id = "dm:alice:bob";
            let (low_id, high_id) = {
                let a = GroupId::generate();
                let b = GroupId::generate();
                if a.as_ref() < b.as_ref() {
                    (a, b)
                } else {
                    (b, a)
                }
            };
            let now = now_ns();

            let restored = StoredGroup::builder()
                .id(high_id)
                .created_at_ns(now)
                .last_message_ns(now)
                .membership_state(GroupMembershipState::Restored)
                .added_by_inbox_id("alice")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            restored.store(conn).unwrap();

            let active = StoredGroup::builder()
                .id(low_id)
                .created_at_ns(now)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("alice")
                .dm_id(Some(dm_id.to_string()))
                .build()
                .unwrap();
            active.store(conn).unwrap();

            let listed = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();
            let dm_rows: Vec<_> = listed
                .iter()
                .filter(|conversation| conversation.dm_id.as_deref() == Some(dm_id))
                .collect();
            assert_eq!(dm_rows.len(), 1);
            assert_eq!(dm_rows[0].id, active.id);
        })
    }

    #[xmtp_common::test]
    fn test_find_conversations_default_excludes_denied() {
        with_connection(|conn| {
            // Create three groups: one allowed, one denied, one unknown (no consent)
            let allowed_group = generate_group(Some(GroupMembershipState::Allowed));
            allowed_group.store(conn).unwrap();

            let denied_group = generate_group(Some(GroupMembershipState::Allowed));
            denied_group.store(conn).unwrap();

            let unknown_group = generate_group(Some(GroupMembershipState::Allowed));
            unknown_group.store(conn).unwrap();

            // Create consent records for allowed and denied; leave unknown_group without one
            let allowed_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(allowed_group.id),
            );
            allowed_consent.store(conn).unwrap();

            let denied_consent = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Denied,
                hex::encode(denied_group.id),
            );
            denied_consent.store(conn).unwrap();

            // Query using default args (no consent_states specified)
            let default_results = conn
                .fetch_conversation_list(GroupQueryArgs::default())
                .unwrap();

            // Expect to include only: allowed_group and unknown_group (2 total)
            assert_eq!(default_results.len(), 2);
            let returned_ids: Vec<_> = default_results.iter().map(|g| &g.id).collect();
            assert!(returned_ids.contains(&&allowed_group.id));
            assert!(returned_ids.contains(&&unknown_group.id));
            assert!(!returned_ids.contains(&&denied_group.id));
        })
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn test_unknown_content_type_is_present() {
        with_connection(|conn| {
            let dm = generate_dm(None);
            dm.store(conn)?;

            let m = generate_message(
                None,
                Some(&dm.id),
                Some(5000),
                Some(ContentType::Unknown),
                None,
                None,
            );
            m.store(conn)?;

            let conv = conn.fetch_conversation_list(GroupQueryArgs {
                ..Default::default()
            })?;

            // Message id should be present
            assert!(conv[0].message_id.is_some());
        })
    }

    #[xmtp_common::test]
    fn test_last_activity_after_ns_filter() {
        with_connection(|conn| {
            // Create groups with specific creation times
            let group1 = generate_group_with_created_at(None, 1000);
            let group2 = generate_group_with_created_at(None, 2000);
            let group3 = generate_group_with_created_at(None, 3000);

            group1.store(conn).unwrap();
            group2.store(conn).unwrap();
            group3.store(conn).unwrap();

            // Add a message to group1 at timestamp 5000
            let message1 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group1.id),
                Some(5000),
                Some(ContentType::Text),
                None,
                None,
            );
            message1.store(conn).unwrap();

            // Add a message to group2 at timestamp 4000
            let message2 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group2.id),
                Some(4000),
                Some(ContentType::Text),
                None,
                None,
            );
            message2.store(conn).unwrap();

            // group3 has no messages, so its activity time is its created_at_ns (3000)

            // Test: last_activity_after_ns = 3500 should return group1 (activity at 5000) and group2 (activity at 4000)
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(3500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                results.len(),
                2,
                "Should return groups with activity after 3500"
            );

            let returned_ids: Vec<_> = results.iter().map(|g| &g.id).collect();
            assert!(
                returned_ids.contains(&&group1.id),
                "Should include group1 (message at 5000)"
            );
            assert!(
                returned_ids.contains(&&group2.id),
                "Should include group2 (message at 4000)"
            );
            assert!(
                !returned_ids.contains(&&group3.id),
                "Should not include group3 (created at 3000)"
            );

            // Test: last_activity_after_ns = 4500 should only return group1
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(4500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 1, "Should return only group1");
            assert_eq!(results[0].id, group1.id, "Should be group1");

            // Test: last_activity_after_ns = 2500 should return all groups
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(2500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 3, "Should return all groups");
        })
    }

    #[xmtp_common::test]
    fn test_last_activity_before_ns_filter() {
        with_connection(|conn| {
            // Create groups with specific creation times
            let group1 = generate_group_with_created_at(None, 1000);
            let group2 = generate_group_with_created_at(None, 2000);
            let group3 = generate_group_with_created_at(None, 3000);

            group1.store(conn).unwrap();
            group2.store(conn).unwrap();
            group3.store(conn).unwrap();

            // Add a message to group1 at timestamp 5000
            let message1 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group1.id),
                Some(5000),
                Some(ContentType::Text),
                None,
                None,
            );
            message1.store(conn).unwrap();

            // Add a message to group2 at timestamp 4000
            let message2 = crate::encrypted_store::group_message::tests::generate_message(
                None,
                Some(&group2.id),
                Some(4000),
                Some(ContentType::Text),
                None,
                None,
            );
            message2.store(conn).unwrap();

            // group3 has no messages, so its activity time is its created_at_ns (3000)

            // Test: last_activity_before_ns = 4500 should return group2 (activity at 4000) and group3 (created at 3000)
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_before_ns: Some(4500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(
                results.len(),
                2,
                "Should return groups with activity before 4500"
            );

            let returned_ids: Vec<_> = results.iter().map(|g| &g.id).collect();
            assert!(
                !returned_ids.contains(&&group1.id),
                "Should not include group1 (message at 5000)"
            );
            assert!(
                returned_ids.contains(&&group2.id),
                "Should include group2 (message at 4000)"
            );
            assert!(
                returned_ids.contains(&&group3.id),
                "Should include group3 (created at 3000)"
            );

            // Test: last_activity_before_ns = 3500 should only return group3
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_before_ns: Some(3500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 1, "Should return only group3");
            assert_eq!(results[0].id, group3.id, "Should be group3");

            // Test: last_activity_before_ns = 5500 should return all groups
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_before_ns: Some(5500),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 3, "Should return all groups");
        })
    }

    #[xmtp_common::test]
    fn test_activity_filters_combined_with_limit() {
        with_connection(|conn| {
            // Create multiple groups with different activity times
            let mut groups = Vec::new();
            for i in 0..5 {
                let group = generate_group_with_created_at(None, (i + 1) * 1000);
                group.store(conn).unwrap();

                // Add a message to each group at different times
                let message = crate::encrypted_store::group_message::tests::generate_message(
                    None,
                    Some(&group.id),
                    Some((100 - i) * 1000), // Messages at 100_000, 99_000, 98_000, etc.
                    Some(ContentType::Text),
                    None,
                    None,
                );
                message.store(conn).unwrap();
                groups.push(group);
            }

            // Test: last_activity_after_ns = 7500 with limit = 2
            // Should return groups with messages at 97_000, 98_000, 99_000, 100_000, but only 2 due to limit
            let results = conn
                .fetch_conversation_list(GroupQueryArgs {
                    last_activity_after_ns: Some(96_000),
                    limit: Some(2),
                    order_by: Some(GroupQueryOrderBy::LastActivity),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(results.len(), 2, "Should return 2 groups due to limit");

            // Results should be ordered by activity (latest first)
            assert_eq!(
                results[0].sent_at_ns.unwrap(),
                100_000,
                "First should be most recent"
            );
            assert_eq!(
                results[1].sent_at_ns.unwrap(),
                99_000,
                "Second should be second most recent"
            );
        })
    }
}

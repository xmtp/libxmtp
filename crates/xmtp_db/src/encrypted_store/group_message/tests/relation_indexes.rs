use super::super::*;
use crate::{Store, group::tests::generate_group, test_utils::with_connection};
use diesel::connection::InstrumentationEvent;
use diesel::sql_types::{BigInt, Binary, Text};
use std::sync::{Arc, Mutex};

#[derive(diesel::QueryableByName)]
struct QueryPlan {
    #[diesel(sql_type = Text)]
    detail: String,
}

#[derive(diesel::QueryableByName)]
struct SqliteVersion {
    #[diesel(sql_type = Text)]
    version: String,
}

#[xmtp_common::test(unwrap_try = true)]
fn sparse_history_relations_seek_selected_ids() {
    with_connection(|conn| {
        let group = generate_group(None);
        group.store(conn).unwrap();
        conn.raw_query(|raw| {
            diesel::sql_query(
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<999) \
                 INSERT INTO groups(id,created_at_ns,membership_state,installations_last_checked,added_by_inbox_id) \
                 SELECT CAST(printf('%016d',x) AS BLOB),0,1,0,'fixture' FROM n",
            ).execute(raw)?;
            diesel::sql_query(
                "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) \
                 INSERT INTO group_messages(id,group_id,decrypted_message_bytes,sent_at_ns,sender_installation_id, \
                 sender_inbox_id,delivery_status,content_type,authority_id,sequence_id,delivery_sequence) \
                 SELECT CAST(printf('%032d',x) AS BLOB),CASE WHEN x<=50000 THEN ? ELSE \
                 CAST(printf('%016d',1+(x%999)) AS BLOB) END,zeroblob(256),x,x'01','fixture',2,1,'xmtp.org',0,x FROM n",
            ).bind::<Binary,_>(group.id.as_ref()).execute(raw)?;
            Ok(())
        }).unwrap();
        let ids = (49901..49951)
            .map(|index| format!("{index:032}").into_bytes())
            .collect::<Vec<_>>();
        let references = ids.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let mut plans = Vec::new();
        for analyzed in [false, true] {
            if analyzed {
                conn.raw_query(|raw| diesel::sql_query("ANALYZE").execute(raw))
                    .unwrap();
            }
            let queries = Arc::new(Mutex::new(Vec::new()));
            let captured = queries.clone();
            conn.raw_query(|raw| {
                raw.set_instrumentation(move |event: InstrumentationEvent<'_>| {
                    if let InstrumentationEvent::StartQuery { query, .. } = event {
                        captured.lock().unwrap().push(query.to_string());
                    }
                });
                Ok(())
            })
            .unwrap();
            let reactions = conn
                .get_inbound_relations(
                    &group.id,
                    &references,
                    RelationQuery::builder()
                        .content_types(Some(vec![ContentType::Reaction]))
                        .build()
                        .unwrap(),
                )
                .unwrap();
            let replies = conn
                .get_inbound_relation_counts(
                    &group.id,
                    &references,
                    RelationQuery::builder()
                        .content_types(Some(vec![ContentType::Reply]))
                        .build()
                        .unwrap(),
                )
                .unwrap();
            assert!(reactions.is_empty() && replies.is_empty());
            let captured = queries.lock().unwrap().clone();
            assert_eq!(captured.len(), 2);
            for (query, content_type) in captured
                .into_iter()
                .zip([ContentType::Reaction, ContentType::Reply])
            {
                let sql = query.split(" -- binds: ").next().unwrap();
                let mut explain = diesel::sql_query(format!("EXPLAIN QUERY PLAN {sql}"))
                    .into_boxed::<Sqlite>()
                    .bind::<Binary, _>(group.id.as_ref())
                    .bind::<Binary, _>(group.id.as_ref());
                for id in &ids {
                    explain = explain.bind::<Binary, _>(id);
                }
                let plan = conn
                    .raw_query(|raw| {
                        explain
                            .bind::<BigInt, _>(now_ns())
                            .bind::<Integer, _>(content_type as i32)
                            .load::<QueryPlan>(raw)
                    })
                    .unwrap()
                    .into_iter()
                    .map(|row| row.detail)
                    .collect::<Vec<_>>();
                let version = conn
                    .raw_query(|raw| {
                        diesel::sql_query("SELECT sqlite_version() AS version")
                            .get_result::<SqliteVersion>(raw)
                    })
                    .unwrap();
                println!(
                    "SDK_RELATION_INDEX_PROOF sqlite={} analyzed={analyzed} content_type={} group={:?}",
                    version.version,
                    content_type as i32,
                    group.id.as_ref()
                );
                println!("SDK_RELATION_SQL {sql}");
                println!("SDK_RELATION_PLAN {plan:?}");
                println!("SDK_RELATION_IDS {ids:?}");
                plans.push(plan.join("\n"));
            }
        }
        for plan in plans {
            assert!(
                plan.contains("group_messages_group_reference")
                    || plan.contains("group_messages_reference_id"),
                "{plan}"
            );
            assert!(!plan.contains("group_messages_sent_at_sort"), "{plan}");
            assert!(!plan.contains("group_messages_inserted_at_sort"), "{plan}");
        }
    });
}

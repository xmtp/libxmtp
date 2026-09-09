//! Real gRPC calls against the local backend and PostgreSQL.
use super::*;
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

#[xmtp_common::test(unwrap_try = true)]
async fn backend_round_trip_covers_five_kinds_paging_and_absent_key() {
    let client = xmtp_api_backend::TestClient::create().build()?;
    let api = ApiClientWrapper::new(client, Retry::default());
    let history = identity_history_with_passkey().await;
    let registration = api
        .publish_identity_update(history.history[0].clone())
        .await?;
    assert!(registration.0 > 0);
    let registered = api.get_envelope(registration.0).await?;
    assert_eq!(
        registered.envelope,
        Some(identity_envelope(history.history[0].clone()))
    );
    let updates = api
        .get_identity_updates_v2(vec![crate::GetIdentityUpdatesV2Filter {
            inbox_id: history.inbox_id.clone(),
            sequence_id: None,
        }])
        .await?;
    assert_eq!(updates[&history.inbox_id].len(), 1);
    assert_eq!(updates[&history.inbox_id][0].update, history.history[0]);

    let key = key_package_envelope(&history.inbox_id, Default::default());
    let key_meta = api.upload_key_package(key.tls_bytes.clone()).await?;
    let key_id: InstallationId = key.installation_id.as_slice().try_into()?;
    let absent: InstallationId = xmtp_common::rand_array::<32>().into();
    let found = api.fetch_key_packages(&[key_id, absent, key_id]).await?;
    assert_eq!(
        found[&key_id].as_ref().unwrap().key_package_tls_serialized,
        key.tls_bytes
    );
    assert_eq!(found[&absent], None);
    assert_eq!(
        api.get_envelope(key_meta.cursor.unwrap().sequence_id)
            .await?
            .envelope,
        Some(key.envelope)
    );

    let welcome = inline_welcome_envelope(&key.installation_id);
    let Some(wire::client_envelope::Payload::WelcomeMessage(welcome_payload)) =
        welcome.payload.clone()
    else {
        unreachable!()
    };
    let welcome_meta = api.send_welcome_messages(&[welcome_payload]).await?;
    let welcomes = api
        .query_welcome_messages(key.installation_id.as_slice())
        .await?;
    assert_eq!(welcomes.len(), 1);
    assert_eq!(
        welcomes[0].sequence_id(),
        welcome_meta[0].cursor.as_ref().unwrap().sequence_id
    );
    assert_eq!(welcomes[0].as_v1().unwrap().data, vec![0x10, 0x11]);

    let group_id = xmtp_common::rand_array::<16>();
    let group_topic = Topic::new_group_message(group_id);
    let group_envelopes: Vec<_> = (0..5u8)
        .map(|index| group_message_envelope(group_id, GroupMessageKind::Application, [index]))
        .collect();
    let units = group_envelopes
        .iter()
        .cloned()
        .map(PublishUnit::single)
        .collect::<crate::Result<Vec<_>>>()?;
    let group_metas = api.send_group_messages(units).await?;
    let before = xmtp_proto::api::HasStats::mls_stats(&api.api_client)
        .query
        .get_count();
    let rows = api
        .query_all(HashMap::from([(group_topic.clone(), Cursor(0))]), 2)
        .await?;
    let after = xmtp_proto::api::HasStats::mls_stats(&api.api_client)
        .query
        .get_count();
    assert_eq!(after - before, 3);
    assert_eq!(rows.len(), group_envelopes.len());
    for (row, envelope) in rows.iter().zip(&group_envelopes) {
        assert_eq!(row.envelope.as_ref(), Some(envelope));
    }
    let groups = api.query_group_messages(group_id.into()).await?;
    assert_eq!(groups.len(), 5);
    assert_eq!(
        groups[0].envelope_hash.as_ref().unwrap(),
        &canonical_envelope(&group_envelopes[0]).hash
    );
    assert_eq!(groups[0].expiry_ns, Some(group_metas[0].expiry_ns));

    let commit = commit_log_envelope(group_id);
    let Some(wire::client_envelope::Payload::CommitLogEntry(entry)) = commit.payload.clone() else {
        unreachable!()
    };
    let commit_metas = api.publish_commit_log(vec![entry.clone()]).await?;
    let logs = api
        .query_commit_log(HashMap::from([(
            Topic::new_commit_log(group_id),
            Cursor(0),
        )]))
        .await?;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].payload, entry);
    assert_eq!(logs[0].meta, commit_metas[0]);
    assert_eq!(logs[0].entry.group_id, group_id);

    let replay = api
        .send_group_messages(vec![PublishUnit::single(group_envelopes[0].clone())?])
        .await?;
    assert_eq!(replay[0], group_metas[0]);
}

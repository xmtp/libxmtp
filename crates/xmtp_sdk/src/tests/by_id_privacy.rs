use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn public_message_by_id_excludes_device_sync_payloads() {
    use prost::Message as _;
    use xmtp_db::group_message::QueryGroupMessage;
    use xmtp_proto::xmtp::device_sync::content::{
        DeviceSyncContent, HmacKeyUpdate, PreferenceUpdate, PreferenceUpdates,
        device_sync_content::Content, preference_update::Update,
    };

    let client = Client::create(crate::generate_local_signer().await, options()).await?;
    let group = client.conversations().create_group(vec![], None).await?;
    let visible_id = group.send_text("public message".into(), None).await?;
    let sync = client.inner.device_sync_client().get_sync_group().await?;
    let key = b"private-sync-hmac-root-canary-32!!".to_vec();
    let payload = DeviceSyncContent {
        content: Some(Content::PreferenceUpdates(PreferenceUpdates {
            updates: vec![PreferenceUpdate {
                update: Some(Update::Hmac(HmacKeyUpdate {
                    key: key.clone(),
                    cycled_at_ns: 1,
                })),
            }],
        })),
    }
    .encode_to_vec();
    let content = crate::EncodedContent {
        r#type: crate::ContentTypeId {
            authority_id: "xmtp.org".into(),
            type_id: "application/x-protobuf".into(),
            version_major: 1,
            version_minor: 0,
        },
        parameters: Default::default(),
        fallback: None,
        content: payload,
    };
    let wire: xmtp_proto::xmtp::mls::message_contents::EncodedContent = content.into();
    let bytes = wire.encode_to_vec();
    let id =
        sync.prepare_message_for_later_publish(&bytes, false, Some("private-sync-lookup".into()))?;
    let stored = client
        .inner
        .context
        .db()
        .get_group_message(&id)?
        .expect("internal sync row");
    assert!(
        stored
            .decrypted_message_bytes
            .windows(key.len())
            .any(|part| part == key)
    );
    let result = client
        .conversations()
        .get_message_by_id(crate::MessageId::from_bytes(&id)?)
        .await?;
    let ordinary = client.conversations().get_message_by_id(visible_id).await?;
    client.end().await?;
    assert!(
        ordinary.is_some(),
        "regular conversation message stays visible"
    );
    assert!(
        result.is_none(),
        "public lookup returned the internal HMAC sync payload"
    );
}

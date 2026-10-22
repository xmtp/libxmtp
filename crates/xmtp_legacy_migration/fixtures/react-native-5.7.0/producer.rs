#![recursion_limit = "256"]

use serde_json::json;
use std::{error::Error, fs, path::PathBuf};
use xmtp_cryptography::ethereum::{
    address_from_pubkey, public_key_uncompressed, sign_recoverable, zeroizing_private_key,
};
use xmtpv3::{
    identity::{FfiIdentifier, FfiIdentifierKind, generate_inbox_id},
    worker::FfiDeviceSyncMode,
    *,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let output = PathBuf::from(std::env::args().nth(1).expect("output directory"));
    let host = std::env::args().nth(2).expect("local backend URL");
    assert!(
        host.starts_with("http://127.0.0.1:"),
        "synthetic producer requires a local backend"
    );
    fs::create_dir_all(&output)?;
    let database = output.join("react-native-5.7.0.db3");
    assert!(!database.exists(), "producer requires an absent database");
    // Public synthetic wallet. Never use this key outside the local fixture backend.
    let wallet_key = [0x37; 32];
    let address = address_from_pubkey(&public_key_uncompressed(zeroizing_private_key(
        &wallet_key,
    )?)?)?;
    let identifier = FfiIdentifier {
        identifier: address,
        identifier_kind: FfiIdentifierKind::Ethereum,
    };
    let nonce = 0;
    let inbox = generate_inbox_id(identifier.clone(), nonce)?;
    let api = connect_to_backend(
        host,
        None,
        false,
        None,
        Some("rn-legacy-fixture".into()),
        None,
        None,
    )
    .await?;
    let client = create_client(
        api.clone(),
        api,
        DbOptions::new(
            Some(database.to_str().unwrap().into()),
            Some(vec![0x53; 32]),
            None,
            None,
        ),
        &inbox,
        identifier,
        nonce,
        None,
        Some(FfiDeviceSyncMode::Disabled),
        None,
        None,
    )
    .await?;
    if let Some(request) = client.signature_request() {
        let text = request.signature_text().await?;
        request
            .add_ecdsa_signature(
                sign_recoverable(text.as_bytes(), zeroizing_private_key(&wallet_key)?, true)?
                    .into(),
            )
            .await?;
        client.register_identity(request).await?;
    }
    let conversations = client.conversations();
    let group = conversations
        .create_group(
            vec![],
            FfiCreateGroupOptions {
                group_name: Some("React Native 5.7.0 history".into()),
                group_description: Some(
                    "Produced by the exact embedded pre-v8 mobile source".into(),
                ),
                ..Default::default()
            },
        )
        .await?;
    let texts = [
        "React Native legacy history: first message",
        "React Native legacy history: second message",
    ];
    let mut expected_messages = vec![];
    for text in texts {
        let id = group
            .send(
                encode_text(text.into())?,
                FfiSendMessageOpts { should_push: false },
            )
            .await?;
        let message = client.message(id.clone())?;
        assert_eq!(decode_text(message.content.clone())?, text);
        expected_messages.push(json!({
            "id": hex::encode(id), "groupId": hex::encode(&message.conversation_id),
            "senderInboxId": message.sender_inbox_id, "sentAtNs": message.sent_at_ns.to_string(),
            "encodedContentHex": hex::encode(message.content), "text": text,
            "expireAtNs": message.expire_at_ns.map(|n| n.to_string())
        }));
    }
    let consent_entity = "a7".repeat(32);
    client
        .set_consent_states(vec![FfiConsent {
            entity_type: FfiConsentEntityType::InboxId,
            state: FfiConsentState::Denied,
            entity: consent_entity.clone(),
        }])
        .await?;
    assert_eq!(
        client
            .get_consent_state(FfiConsentEntityType::InboxId, consent_entity.clone())
            .await?,
        FfiConsentState::Denied
    );
    assert_eq!(
        client
            .get_consent_state(
                FfiConsentEntityType::ConversationId,
                hex::encode(group.id())
            )
            .await?,
        FfiConsentState::Allowed
    );
    let expected = json!({
        "schema": 1,
        "counts": {"groups": 1, "messages": 2, "consent": 2},
        "databaseKeyHex": "53".repeat(32),
        "groups": [{"id": hex::encode(group.id()), "name": group.group_name()?, "description": group.group_description()?, "createdAtNs": group.created_at_ns().to_string()}],
        "messages": expected_messages,
        "consent": [{"entityType": "conversation_id", "entity": hex::encode(group.id()), "state": "allowed"}, {"entityType": "inbox_id", "entity": consent_entity, "state": "denied"}]
    });
    fs::write(
        output.join("expected-records.json"),
        serde_json::to_string_pretty(&expected)? + "\n",
    )?;
    drop(group);
    drop(conversations);
    client.release_db_connection()?;
    drop(client);
    Ok(())
}

use crate::ContentCodec;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use prost::Message;
use serde_json::Value;
use std::fs;
use std::path::Path;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

/// Verifies JSON content equality between original and re-encoded content for specific content types.
/// Handles special cases like networkId normalization for transactionReference.
fn verify_json_content_equality(
    content_type: &str,
    original_content: &[u8],
    re_encoded_content: &[u8],
    index: usize,
    description: &str,
) {
    let mut original_json: Value =
        serde_json::from_slice(original_content).unwrap_or_else(|_| Value::Null);
    let re_encoded_json: Value =
        serde_json::from_slice(re_encoded_content).unwrap_or_else(|_| Value::Null);

    // For transactionReference, normalize networkId to handle number->string conversion
    if content_type == "transactionReference" {
        if let Some(obj) = original_json.as_object_mut() {
            if let Some(network_id) = obj.get("networkId") {
                if let Some(n) = network_id.as_u64() {
                    obj.insert("networkId".to_string(), Value::String(n.to_string()));
                }
            }
        }
    }

    assert_eq!(
        original_json,
        re_encoded_json,
        "JSON content mismatch for {} ({}): {}",
        content_type,
        index + 1,
        description
    );
}

/// Decodes and re-encodes content using the appropriate codec
fn decode_and_reencode(
    content_type: &str,
    encoded_content: &EncodedContent,
) -> Result<EncodedContent, Box<dyn std::error::Error>> {
    match content_type {
        "text" => {
            use crate::text::TextCodec;
            let content = TextCodec::decode(encoded_content.clone())?;
            Ok(TextCodec::encode(content)?)
        }
        "reaction" => {
            use crate::reaction::LegacyReactionCodec;
            let content = LegacyReactionCodec::decode(encoded_content.clone())?;
            Ok(LegacyReactionCodec::encode(content)?)
        }
        "reply" => {
            use crate::reply::ReplyCodec;
            let content = ReplyCodec::decode(encoded_content.clone())?;
            Ok(ReplyCodec::encode(content)?)
        }
        "readReceipt" => {
            use crate::read_receipt::ReadReceiptCodec;
            let content = ReadReceiptCodec::decode(encoded_content.clone())?;
            Ok(ReadReceiptCodec::encode(content)?)
        }
        "remoteAttachment" => {
            use crate::remote_attachment::RemoteAttachmentCodec;
            let content = RemoteAttachmentCodec::decode(encoded_content.clone())?;
            Ok(RemoteAttachmentCodec::encode(content)?)
        }
        "transactionReference" => {
            use crate::transaction_reference::TransactionReferenceCodec;
            let content = TransactionReferenceCodec::decode(encoded_content.clone())?;
            Ok(TransactionReferenceCodec::encode(content)?)
        }
        "groupUpdated" => {
            use crate::group_updated::GroupUpdatedCodec;
            let content = GroupUpdatedCodec::decode(encoded_content.clone())?;
            Ok(GroupUpdatedCodec::encode(content)?)
        }
        "attachment" => {
            use crate::attachment::AttachmentCodec;
            let content = AttachmentCodec::decode(encoded_content.clone())?;
            Ok(AttachmentCodec::encode(content)?)
        }
        _ => Err(format!("Unsupported content type: {content_type}").into()),
    }
}

#[test]
fn integration_test() {
    let fixtures_path = Path::new("fixtures/serialized_content.json");
    let json_content =
        fs::read_to_string(fixtures_path).expect("Failed to read serialized_content.json");

    let examples: Vec<Value> =
        serde_json::from_str(&json_content).expect("Failed to parse JSON content");

    for (index, example) in examples.iter().enumerate() {
        let content_type = example["contentType"]
            .as_str()
            .expect("Missing contentType field");
        let description = example["description"]
            .as_str()
            .expect("Missing description field");
        let encoded_content_b64 = example["encodedContent"]
            .as_str()
            .expect("Missing encodedContent field");

        // Skip content types that don't have codec implementations yet
        if matches!(content_type, "walletSendCalls" | "markdown") {
            continue;
        }

        // Base64 decode and parse as EncodedContent
        let decoded_bytes = BASE64
            .decode(encoded_content_b64)
            .expect("Failed to base64 decode encodedContent");
        let encoded_content = EncodedContent::decode(&mut decoded_bytes.as_slice())
            .expect("Failed to decode as EncodedContent");

        // Decode and re-encode the content
        let decoded_value = decode_and_reencode(content_type, &encoded_content)
            .unwrap_or_else(|_| panic!("Failed to process {content_type} content"));

        // Verify content type preservation
        assert_eq!(
            encoded_content.r#type,
            decoded_value.r#type,
            "Content type mismatch for {} ({}): {}",
            content_type,
            index + 1,
            description
        );

        // Verify parameters match between original and re-encoded content
        assert_eq!(
            encoded_content.parameters,
            decoded_value.parameters,
            "Parameters mismatch for {} ({}): {}",
            content_type,
            index + 1,
            description
        );

        // Verify fallback matches between original and re-encoded content
        if encoded_content.fallback.is_some() {
            assert_eq!(
                encoded_content.fallback,
                decoded_value.fallback,
                "Fallback mismatch for {} ({}): {}",
                content_type,
                index + 1,
                description
            );
        }

        // Verify JSON equality for content types that store JSON
        if matches!(content_type, "transactionReference" | "reaction") {
            verify_json_content_equality(
                content_type,
                &encoded_content.content,
                &decoded_value.content,
                index,
                description,
            );
        }

        // Verify content is not empty (except for ReadReceipt)
        if content_type != "readReceipt" {
            assert!(
                !decoded_value.content.is_empty(),
                "Empty content for {} ({}): {}",
                content_type,
                index + 1,
                description
            );
        }
    }
}

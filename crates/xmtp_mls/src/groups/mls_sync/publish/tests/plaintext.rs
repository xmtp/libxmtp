//! A finished `SendMessage` intent keeps no plaintext.

use super::faults::refusing_group;
use super::send_state_transitions::{intents_in, status};
use super::*;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
use xmtp_db::group_message::DeliveryStatus;

const SECRET: &str = "plaintext-marker-7f3a";

fn secret() -> Vec<u8> {
    encoded_content_to_bytes(TextCodec::encode(SECRET.into()).unwrap())
}

/// Whether any stored message or intent of `group` still holds the marker.
fn holds_plaintext<C: XmtpSharedContext>(group: &MlsGroup<C>) -> bool {
    let marker = SECRET.as_bytes();
    let contains = |bytes: &[u8]| bytes.windows(marker.len()).any(|window| window == marker);
    let intents = group
        .context
        .db()
        .find_group_intents(group.group_id, None, None)
        .unwrap();
    let messages = group.find_messages(&Default::default()).unwrap();
    intents
        .iter()
        .any(|intent| contains(intent.data.as_slice()))
        || messages
            .iter()
            .any(|message| contains(&message.decrypted_message_bytes))
}

/// How the send intent finishes.
#[derive(Clone, Copy, Debug)]
enum Finish {
    /// Published and processed.
    Processed,
    /// Refused by the backend.
    Refused,
    /// The group became inactive with the send queued.
    Inactive,
}

// verifies: PROC-037
#[rstest::rstest]
#[case::processed(Finish::Processed)]
#[case::refused(Finish::Refused)]
#[case::inactive(Finish::Inactive)]
#[xmtp_common::test(unwrap_try = true)]
async fn sent_plaintext_is_wiped(#[case] finish: Finish) -> Result<(), Box<dyn std::error::Error>> {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message = group.send_message_optimistic(&secret(), Default::default())?;
    let terminal = match finish {
        Finish::Processed => {
            group.publish_messages().await?;
            IntentState::Processed
        }
        Finish::Refused => {
            let refusing =
                refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;
            assert!(refusing.publish_intents().await.is_err());
            // The row keeps its content so the app can retry it.
            assert_eq!(status(&group, &message), DeliveryStatus::Failed);
            assert_eq!(
                alix.context
                    .db()
                    .get_group_message(&message)?
                    .unwrap()
                    .decrypted_message_bytes,
                secret()
            );
            IntentState::Error
        }
        Finish::Inactive => {
            alix.context
                .db()
                .supersede_pending_intents_for_inactive_group(group.group_id.as_ref())?;
            IntentState::Error
        }
    };
    let finished = intents_in(&group, &[terminal], IntentKind::SendMessage);
    assert_eq!(finished.len(), 1, "{finish:?}");
    assert!(finished[0].data.as_slice().is_empty(), "{finish:?}");
    Ok(())
}

/// A send fails, its retry succeeds, and the message is deleted for
/// everyone: no message or intent row keeps its plaintext.
// verifies: PROC-037
#[xmtp_common::test(unwrap_try = true)]
async fn deleted_retried_send_keeps_no_plaintext() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    group.key_update().await?;
    let message = group.send_message_optimistic(&secret(), Default::default())?;
    let refusing = refusing_group(&alix, &group.group_id, [tonic::Code::InvalidArgument]).await;
    assert!(refusing.publish_intents().await.is_err());
    group.publish_stored_message(&message).await?;
    assert_eq!(status(&group, &message), DeliveryStatus::Published);
    assert!(holds_plaintext(&group));

    group.delete_message(message.clone())?;
    group.publish_messages().await?;
    assert!(!holds_plaintext(&group));
}

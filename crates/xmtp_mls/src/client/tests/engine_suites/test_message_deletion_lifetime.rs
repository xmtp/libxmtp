//! A deletion record outlives the deletion message that made it.

use crate::context::CoreContext;
use crate::messages::decoded_message::{DeletedBy, MessageBody};
use crate::tester;
use crate::worker::disappearing_messages::{
    DisappearingMessagesWorker, test_support::delete_expired_messages,
};
use xmtp_common::time::now_ns;
use xmtp_db::prelude::*;

/// Encoded text with a fallback, so a test can see whether either leaks.
fn secret_text() -> Vec<u8> {
    use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
    let mut secret = TextCodec::encode("secret".into()).unwrap();
    secret.fallback = Some("secret fallback".into());
    encoded_content_to_bytes(secret)
}

/// How the deletion message goes before its target.
#[derive(Clone, Copy, Debug)]
enum DeletionGoes {
    /// It expires; the target has no expiry.
    Expires,
    /// It expires; the target expires later.
    ExpiresBeforeTarget,
    /// The app removes it locally.
    RemovedLocally,
}

/// A mark lives and dies with its target: the deletion message's expiry
/// or local removal leaves the placeholder, and the record stays.
// verifies: PROC-037, META-063
#[rstest::rstest]
#[case::expires(DeletionGoes::Expires)]
#[case::expires_before_target(DeletionGoes::ExpiresBeforeTarget)]
#[case::removed_locally(DeletionGoes::RemovedLocally)]
#[xmtp_common::test(unwrap_try = true)]
async fn deletion_outlives_its_message(
    #[case] goes: DeletionGoes,
) -> Result<(), Box<dyn std::error::Error>> {
    use xmtp_db::ConnectionExt;
    use xmtp_db::diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::message_deletion::QueryMessageDeletion;
    use xmtp_db::schema::group_messages::dsl;

    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let target = group
        .send_message(&secret_text(), Default::default())
        .await?;
    let deletion = group.delete_message(target.clone())?;
    group.publish_messages().await?;
    let db = alix.context.db();
    let expire = |id: &[u8], at: i64| {
        db.raw_query(|conn| {
            xmtp_db::diesel::update(dsl::group_messages.filter(dsl::id.eq(id)))
                .set(dsl::expire_at_ns.eq(Some(at)))
                .execute(conn)
        })
        .unwrap();
    };
    let later = now_ns() + 60 * 1_000_000_000;
    match goes {
        DeletionGoes::Expires => expire(&deletion, now_ns() - 1),
        DeletionGoes::ExpiresBeforeTarget => {
            expire(&deletion, now_ns() - 1);
            expire(&target, later);
        }
        DeletionGoes::RemovedLocally => {
            assert_eq!(alix.delete_message(deletion.clone())?, 1);
        }
    }
    let mut worker = DisappearingMessagesWorker::new(alix.context.clone());
    delete_expired_messages(&mut worker).await?;
    assert!(db.get_group_message(&deletion)?.is_none());
    assert!(db.get_message_deletion(&deletion)?.is_some());
    let read = group
        .find_messages_v2(&Default::default())?
        .into_iter()
        .find(|message| message.metadata.id == target)
        .expect("the placeholder stays");
    assert!(matches!(
        read.content,
        MessageBody::DeletedMessage {
            deleted_by: DeletedBy::Sender
        }
    ));
    if let DeletionGoes::ExpiresBeforeTarget = goes {
        expire(&target, now_ns() - 1);
        delete_expired_messages(&mut worker).await?;
        assert!(db.get_group_message(&target)?.is_none());
    }
    Ok(())
}

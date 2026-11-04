//! Who may delete which message, and what every installation then shows.

use crate::groups::UpdateAdminListType;
use crate::groups::send_message_opts::SendMessageOpts;
use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use EngineTestGroup as Group;
use xmtp_content_types::delete_message::DeleteMessageCodec;
use xmtp_content_types::{ContentCodec, text::TextCodec};
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs, QueryGroupMessage};
use xmtp_mls_messages::decoded_message::{DeletedBy, MessageBody};
use xmtp_proto::xmtp::mls::message_contents::content_types::DeleteMessage;

#[derive(Debug, Clone, Copy)]
enum Case {
    SenderDeletesOwn,
    AdminDeletesOther,
    MemberDeletesOther,
    AdminDemotedFirst,
    MemberPromotedFirst,
    OtherGroupId,
    Transcript,
    OwnDeletionUnpublished,
    RawDeleteBySender,
    SenderThenAdmin,
}

fn text(body: &str) -> Vec<u8> {
    xmtp_content_types::encoded_content_to_bytes(TextCodec::encode(body.into()).unwrap())
}

fn raw_delete(target: &[u8]) -> Vec<u8> {
    xmtp_content_types::encoded_content_to_bytes(
        DeleteMessageCodec::encode(DeleteMessage {
            message_id: hex::encode(target),
        })
        .unwrap(),
    )
}

/// Who marked `id` on this installation, whether its bytes are empty, and
/// what the app reads for it.
fn view(group: &Group, id: &[u8]) -> Option<(Option<String>, bool, Option<DeletedBy>)> {
    let stored = group.context.db().get_group_message(id).unwrap()?;
    // History lists application messages only.
    let read = group
        .find_messages_v2(&MsgQueryArgs::default())
        .unwrap()
        .into_iter()
        .find(|message| message.metadata.id == id);
    let placeholder = read.and_then(|read| match read.content {
        MessageBody::DeletedMessage { deleted_by } => Some(deleted_by),
        _ => None,
    });
    Some((
        stored.deleted_by_inbox_id,
        stored.decrypted_message_bytes.is_empty(),
        placeholder,
    ))
}

/// Alix creates the group and is its super admin; bo and caro are members.
/// Bo sends the target unless the case says otherwise.
// verifies: PROC-037, PROC-060, CTYPE-018
#[rstest::rstest]
#[case::sender_deletes_own(Case::SenderDeletesOwn)]
#[case::admin_deletes_other(Case::AdminDeletesOther)]
#[case::member_deletes_other(Case::MemberDeletesOther)]
#[case::admin_demoted_first(Case::AdminDemotedFirst)]
#[case::member_promoted_first(Case::MemberPromotedFirst)]
#[case::other_group_id(Case::OtherGroupId)]
#[case::transcript(Case::Transcript)]
#[case::own_deletion_unpublished(Case::OwnDeletionUnpublished)]
#[case::raw_delete_by_sender(Case::RawDeleteBySender)]
#[case::sender_then_admin(Case::SenderThenAdmin)]
#[xmtp_common::test]
async fn deletion_rules(#[case] case: Case) {
    use xmtp_mls_core::context::CoreContext;
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .unwrap();
    let bo_group = bo.wait_for_welcomes().await.unwrap().remove(0);
    let caro_group = caro.wait_for_welcomes().await.unwrap().remove(0);
    let opts = SendMessageOpts::default;
    let target = if matches!(case, Case::Transcript) {
        alix_group.sync().await.unwrap();
        alix_group
            .find_messages(&MsgQueryArgs {
                kind: Some(GroupMessageKind::MembershipChange),
                ..Default::default()
            })
            .unwrap()[0]
            .id
            .clone()
    } else {
        bo_group
            .send_message(&text("target"), opts())
            .await
            .unwrap()
    };
    for group in [&alix_group, &bo_group, &caro_group] {
        group.sync().await.unwrap();
    }
    let events = caro.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::MessageDeleted]),
        Some(10),
    );
    let bo_inbox = bo.inbox_id().to_string();
    let alix_inbox = alix.inbox_id().to_string();
    let caro_inbox = caro.inbox_id().to_string();
    let expected: Option<(String, DeletedBy)> = match case {
        Case::SenderDeletesOwn => {
            bo_group.delete_message(target.clone()).unwrap();
            bo_group.publish_messages().await.unwrap();
            Some((bo_inbox.clone(), DeletedBy::Sender))
        }
        Case::AdminDeletesOther => {
            alix_group.delete_message(target.clone()).unwrap();
            alix_group.publish_messages().await.unwrap();
            Some((alix_inbox.clone(), DeletedBy::Admin(alix_inbox.clone())))
        }
        Case::MemberDeletesOther => {
            // The request refuses, so the member sends the content directly.
            assert!(caro_group.delete_message(target.clone()).is_err());
            caro_group
                .send_message(&raw_delete(&target), opts())
                .await
                .unwrap();
            None
        }
        Case::AdminDemotedFirst => {
            alix_group
                .update_admin_list(UpdateAdminListType::AddSuper, caro_inbox.clone())
                .await
                .unwrap();
            caro_group.sync().await.unwrap();
            // Queued while a super admin; the demotion is ordered first.
            caro_group.delete_message(target.clone()).unwrap();
            alix_group
                .update_admin_list(UpdateAdminListType::RemoveSuper, caro_inbox.clone())
                .await
                .unwrap();
            caro_group.publish_messages().await.unwrap();
            None
        }
        Case::MemberPromotedFirst => {
            caro_group
                .send_message_optimistic(&raw_delete(&target), opts())
                .unwrap();
            alix_group
                .update_admin_list(UpdateAdminListType::AddSuper, caro_inbox.clone())
                .await
                .unwrap();
            caro_group.publish_messages().await.unwrap();
            Some((caro_inbox.clone(), DeletedBy::Admin(caro_inbox.clone())))
        }
        Case::OtherGroupId => {
            // Bo deletes its own message from another group it shares.
            let other = bo
                .create_group_with_members(&[alix.inbox_id(), caro.inbox_id()], None, None)
                .await
                .unwrap();
            other
                .send_message(&raw_delete(&target), opts())
                .await
                .unwrap();
            alix.sync_all_welcomes_and_groups(None).await.unwrap();
            caro.sync_all_welcomes_and_groups(None).await.unwrap();
            None
        }
        Case::Transcript => {
            alix_group
                .send_message(&raw_delete(&target), opts())
                .await
                .unwrap();
            None
        }
        Case::OwnDeletionUnpublished => {
            // The target must stay readable: see the case assertions below.
            bo_group.delete_message(target.clone()).unwrap();
            None
        }
        Case::RawDeleteBySender => {
            bo_group
                .send_message(&raw_delete(&target), opts())
                .await
                .unwrap();
            Some((bo_inbox.clone(), DeletedBy::Sender))
        }
        Case::SenderThenAdmin => {
            bo_group.delete_message(target.clone()).unwrap();
            bo_group.publish_messages().await.unwrap();
            alix_group.sync().await.unwrap();
            alix_group
                .send_message(&raw_delete(&target), opts())
                .await
                .unwrap();
            Some((bo_inbox.clone(), DeletedBy::Sender))
        }
    };
    for group in [&alix_group, &caro_group] {
        group.sync().await.unwrap();
    }
    if !matches!(case, Case::OwnDeletionUnpublished) {
        bo_group.sync().await.unwrap();
    }
    for (name, group) in [
        ("alix", &alix_group),
        ("bo", &bo_group),
        ("caro", &caro_group),
    ] {
        // A membership change row is local to each installation.
        let Some((mark, empty, placeholder)) = view(group, &target) else {
            assert!(matches!(case, Case::Transcript) && name != "alix");
            continue;
        };
        match &expected {
            Some((by, deleted_by)) => {
                assert_eq!(mark.as_ref(), Some(by), "{case:?} {name}");
                assert!(empty, "{case:?} {name}: content kept");
                assert_eq!(placeholder.as_ref(), Some(deleted_by), "{case:?} {name}");
            }
            None => {
                assert_eq!(mark, None, "{case:?} {name}");
                assert!(!empty, "{case:?} {name}: content removed");
                assert_eq!(placeholder, None, "{case:?} {name}");
            }
        }
    }
    let deleted = events
        .drain()
        .into_iter()
        .filter(|event| {
            matches!(&event.client, Some(xmtp_events::ClientEvent::MessageDeleted(d))
                if d.message_id == target)
        })
        .count();
    assert_eq!(deleted, usize::from(expected.is_some()), "{case:?} events");
}

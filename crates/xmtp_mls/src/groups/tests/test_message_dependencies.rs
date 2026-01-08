use xmtp_api_d14n::protocol::{XmtpEnvelope, XmtpQuery};
use xmtp_db::prelude::QueryGroupMessage;
use xmtp_proto::types::{Cursor, TopicKind};

use crate::groups::MlsGroup;
use crate::utils::test::MlsGroupExt;
use crate::{context::XmtpSharedContext, tester};

// gets all messages on a group topic
async fn get_messages(context: &impl XmtpSharedContext, group_id: &[u8]) -> XmtpEnvelope {
    context
        .api()
        .query_at(TopicKind::GroupMessagesV1.create(group_id), None)
        .await
        .unwrap()
}

fn message_debug(env: &XmtpEnvelope) -> String {
    env.group_messages().unwrap().into_iter().enumerate().fold(
        String::new(),
        |mut acc, (i, msg)| {
            acc.push_str(&i.to_string());
            acc.push_str(" --");
            acc.push_str(&msg.to_string());
            acc.push('\n');
            acc
        },
    )
}

fn db_message_debug(db: impl QueryGroupMessage, id: &[u8]) -> String {
    db.get_group_messages(id, &Default::default())
        .unwrap()
        .into_iter()
        .enumerate()
        .fold(String::new(), |mut acc, (i, msg)| {
            acc.push_str(&i.to_string());
            acc.push_str(" --");
            acc.push_str(&msg.cursor().to_string());
            acc.push('\n');
            acc
        })
}

async fn sync<C: XmtpSharedContext>(groups: &[&MlsGroup<C>]) {
    for g in groups {
        let _ = g.sync().await.unwrap();
    }
}

// ensure `dependant` depends on `commit`
#[track_caller]
fn assert_depends_on(env: &XmtpEnvelope, dependant: usize, commit: usize) {
    let client_envelopes = env.client_envelopes().unwrap();
    let cursors = env.cursors().unwrap();
    let depends_on_cursor: Cursor = client_envelopes[dependant]
        .aad
        .as_ref()
        .unwrap()
        .depends_on
        .as_ref()
        .unwrap()
        .clone()
        .try_into()
        .unwrap();
    assert_eq!(
        depends_on_cursor,
        cursors[commit],
        "envelope [{}] has dependency {}, but expected dependency {}\n{}",
        cursors[dependant],
        client_envelopes[dependant].aad.as_ref().unwrap(),
        cursors[commit],
        message_debug(env)
    );
}

#[track_caller]
fn assert_no_dependant(env: &XmtpEnvelope, msg: usize) {
    let client_envelopes = env.client_envelopes().unwrap();
    let depends_on = &client_envelopes[msg].aad.as_ref().unwrap().depends_on;
    assert!(depends_on.is_none())
}

#[xmtp_common::test(unwrap_try = true)]
async fn messages_have_dependencies() {
    tester!(alix);
    tester!(bo);

    let alix_group = alix.create_group(None, None)?;
    let group_id = alix_group.group_id.clone();
    alix_group.invite(&bo).await?;
    let messages = get_messages(&alix.context, &group_id).await;
    // no messages have been sent in group yet. alix about to process first commit alix
    // sends(inviting bo)
    assert_no_dependant(&messages, 0);
    let bo_group = bo.sync_welcomes().await?.pop()?;
    assert_eq!(bo_group.group_id, group_id);
    bo_group.send_msg(b"2").await;
    let messages = get_messages(&alix.context, &group_id).await;
    // bos key update depends on the first commit (group invite)
    assert_depends_on(&messages, 1, 0);
    // bos message depends on the key update in the group
    assert_depends_on(&messages, 2, 1);
}

#[xmtp_common::test(unwrap_try = true)]
async fn messages_dependencies_out_of_order_invites() {
    tester!(alix, with_name: "alix");
    tester!(bo, with_name: "bo");
    tester!(caro, with_name: "caro");

    let alix_group = alix
        .create_group_with_members(&[bo.inbox_id()], None, None) // message 0
        .await?;
    let group_id = alix_group.group_id.clone();
    let messages = get_messages(&alix.context, &group_id).await;
    assert_no_dependant(&messages, 0);
    let bo_group = bo.sync_welcomes().await?.pop()?;
    assert_eq!(bo_group.group_id, group_id);
    bo_group.send_msg(b"2").await; // message 1 (key update) and 2 (application)

    let messages = get_messages(&alix.context, &group_id).await;
    // bos key update depends on the first commit (group invite)
    assert_depends_on(&messages, 1, 0);
    // bos message depends on the key update in the group
    assert_depends_on(&messages, 2, 1);

    bo_group.invite(&caro).await?; // message 3
    // alix's message has a dependency on the first commit in the group (adding bo)
    // we haven't seen commit adding caro yet.
    alix_group.send_msg(b"3").await; // message 4 (caro will not see)
    let messages = get_messages(&alix.context, &group_id).await;
    // bos add member commit depends on key update
    assert_depends_on(&messages, 3, 1);
    // alixs send message depends on first commit
    assert_depends_on(&messages, 4, 0);
    let caro_group = caro.sync_welcomes().await?.pop()?;
    caro_group.send_msg(b"4").await; // message 5 (application)
    let messages = get_messages(&alix.context, &group_id).await;

    // caro depends on bos commit
    assert_depends_on(&messages, 5, 3);
    // now everyone should be synced to the latest state
    // this will make alix do a key update as well
    sync(&[&alix_group, &bo_group, &caro_group]).await;
    let messages = get_messages(&alix.context, &group_id).await;
    assert_depends_on(&messages, 5, 3);
    alix_group.send_msg(b"5").await; // message 7
    sync(&[&alix_group, &bo_group, &caro_group]).await;
    let messages = get_messages(&alix.context, &group_id).await;
    assert_depends_on(&messages, 6, 5);
    assert_depends_on(&messages, 7, 5);
    // alixs new message depends on bos commit adding caro

    println!("\n====NETWORK====");
    let messages = get_messages(&alix.context, &group_id).await;
    println!("{}", message_debug(&messages));
    println!("\n====ALIX====");
    println!("{}", db_message_debug(alix.db(), &group_id));
    println!("\n====CARO====");
    println!("{}", db_message_debug(caro.db(), &group_id));
    println!("\n====BO====");
    println!("{}", db_message_debug(bo.db(), &group_id));
}

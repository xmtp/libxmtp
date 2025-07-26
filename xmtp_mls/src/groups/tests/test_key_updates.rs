use crate::tester;
use futures::future::join_all;
use std::{future::Future, pin::Pin, time::Duration};
use xmtp_common::{retry_async, Retry};
use xmtp_db::events::Events;

#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_key_rotation_with_optimistic_send() {
    tester!(alix, stream);
    tester!(bo, stream, events);
    let g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
        .await?;

    bo.sync_welcomes().await?;
    let bo_g = bo.group(&g.group_id)?;

    let mut futs = vec![];
    for _ in 0..4 {
        let fut = async {
            g.send_message_optimistic(b"hello there")?;
        };
        let fut = Box::pin(fut) as Pin<Box<dyn Future<Output = ()>>>;
        futs.push(fut);

        let fut = async {
            bo_g.send_message_optimistic(b"hello there")?;
        };
        let fut = Box::pin(fut) as Pin<Box<dyn Future<Output = ()>>>;
        futs.push(fut);
    }

    join_all(futs).await;

    // Wait for the streams to finish.
    xmtp_common::time::sleep(Duration::from_secs(5)).await;

    retry_async!(
        Retry::default(),
        (async { g.test_can_talk_with(&bo_g).await })
    )?;

    let key_updates = Events::key_updates(&bo.context.db())?;
    assert_eq!(key_updates.len(), 1);
}

#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn key_update_out_of_epoch() {
    // Have bo join a group and immediately send several optimistic messages.
    // Add enough people to move the epoch ahead 5, then sync to ensure proper delivery.
    tester!(alix);
    tester!(carl);
    tester!(dre);
    tester!(ed);
    tester!(fester);
    tester!(greg);
    tester!(bo, events);

    let g = alix
        .create_group_with_inbox_ids(&[bo.inbox_id().to_string()], None, None)
        .await?;

    bo.sync_welcomes().await?;
    let bo_g = bo.group(&g.group_id)?;
    for _ in 0..10 {
        bo_g.send_message_optimistic(b"hello there")?;
    }

    g.add_members_by_inbox_id(&[carl.inbox_id().to_string()])
        .await?;
    g.add_members_by_inbox_id(&[dre.inbox_id().to_string()])
        .await?;
    g.add_members_by_inbox_id(&[ed.inbox_id().to_string()])
        .await?;
    g.add_members_by_inbox_id(&[fester.inbox_id().to_string()])
        .await?;
    g.add_members_by_inbox_id(&[greg.inbox_id().to_string()])
        .await?;

    bo_g.sync().await?;
    g.test_can_talk_with(&bo_g).await?;

    let key_updates = Events::key_updates(&bo.context.db())?;

    assert_eq!(key_updates.len(), 1);
}

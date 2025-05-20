use crate::{groups::GroupMetadataOptions, tester};
use futures::future::join_all;
use std::{future::Future, pin::Pin};

#[xmtp_common::test(unwrap_try = "true")]
async fn test_key_rotation_with_optimistic_send() {
    tester!(alix, stream);
    tester!(bo, stream);
    let g = alix
        .create_group_with_inbox_ids(
            &[bo.inbox_id().to_string()],
            None,
            GroupMetadataOptions::default(),
        )
        .await?;

    bo.sync_welcomes().await?;
    let bo_g = bo.group(&g.group_id)?;

    let mut futs = vec![];
    for _ in 0..10 {
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

    g.sync().await?;
    bo_g.sync().await?;

    g.test_can_talk_with(&bo_g).await?;
}

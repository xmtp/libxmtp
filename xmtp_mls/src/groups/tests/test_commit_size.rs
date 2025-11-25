use tracing::Instrument;

use crate::tester;

#[xmtp_common::test]
async fn test_commit_size_one_at_a_time() {
    tester!(alix);
    let g = alix.create_group(None, None).unwrap();

    let mut testers = vec![];
    let mut installations = vec![];
    for _ in 0..1000 {
        tester!(tester);
        g.add_members_by_inbox_id(&[tester.inbox_id()])
            .await
            .unwrap();
        for _ in 0..9 {
            installations.push(tester.new_installation().await);
            g.update_installations().await.unwrap();
        }
        testers.push(tester);
    }
    panic!();
}

#[xmtp_common::test]
async fn test_commit_size_ten_at_a_time() {
    tester!(alix);
    let g = alix.create_group(None, None).unwrap();

    let mut testers = vec![];
    let mut installations = vec![];
    for _ in 0..1000 {
        tester!(tester);
        for _ in 0..9 {
            installations.push(tester.new_installation().await);
            // g.update_installations().await.unwrap();
        }
        g.add_members_by_inbox_id(&[tester.inbox_id()])
            .await
            .unwrap();
        testers.push(tester);
    }
    panic!();
}

#[xmtp_common::test]
async fn test_commit_size_one_at_a_time_with_messages() {
    tester!(alix);
    let g = alix.create_group(None, None).unwrap();

    let mut testers = vec![];
    let mut installations = vec![];
    for i in 0..2 {
        tester!(tester);
        g.add_members_by_inbox_id(&[tester.inbox_id()])
            .await
            .unwrap();
        g.load_mls_group_with_lock_async(|mls| async move {
            println!("tester {i}");
            mls.print_ratchet_tree("");
            dbg!(mls.export_ratchet_tree());
            Ok::<_, crate::groups::GroupError>(())
        })
        .await
        .unwrap();
        let groups = tester.sync_welcomes().await.unwrap();
        groups[0]
            .send_message(b"hello", Default::default())
            .await
            .unwrap();
        g.sync().await.unwrap();
        g.load_mls_group_with_lock_async(|mls| async move {
            println!("tester {i} after sending message");
            mls.print_ratchet_tree("");
            dbg!(mls.export_ratchet_tree());
            Ok::<_, crate::groups::GroupError>(())
        })
        .await
        .unwrap();
        for j in 0..9 {
            let new_installation = tester.new_installation().await;
            g.update_installations().await.unwrap();
            g.load_mls_group_with_lock_async(|mls| async move {
                println!("tester {i} installation {j}");
                mls.print_ratchet_tree("");
                dbg!(mls.export_ratchet_tree());
                Ok::<_, crate::groups::GroupError>(())
            })
            .await
            .unwrap();
            let groups = new_installation.sync_welcomes().await.unwrap();
            groups[0]
                .send_message(b"hello", Default::default())
                .await
                .unwrap();
            g.sync().await.unwrap();
            g.load_mls_group_with_lock_async(|mls| async move {
                println!("tester {i} installation {j} after sending message");
                mls.print_ratchet_tree("");
                dbg!(mls.export_ratchet_tree());
                Ok::<_, crate::groups::GroupError>(())
            })
            .await
            .unwrap();
            installations.push(new_installation);
        }
        testers.push(tester);
    }
}

#[xmtp_common::test]
async fn test_commit_size_ten_at_a_time_with_messages() {
    let installation_count = std::env::var("XMTP_MLSTEST_INSTALLATIONS")
        .unwrap_or("0".to_string())
        .parse::<usize>()
        .unwrap();
    let tester_count = std::env::var("XMTP_MLSTEST_TESTERS")
        .unwrap_or("0".to_string())
        .parse::<usize>()
        .unwrap();
    tester!(alix);
    let g = alix.create_group(None, None).unwrap();

    let mut testers = vec![];
    let mut installations = vec![];
    for i in 0..tester_count {
        tester!(tester);
        let mut new_installations = vec![];
        for j in 0..installation_count {
            new_installations.push(
                tester
                    .new_installation()
                    .instrument(tracing::error_span!("tester installation", i, j))
                    .await,
            );
            // g.update_installations().instrument(tracing::error_span!("g updating installations")).await.unwrap();
        }
        g.load_mls_group_with_lock_async(|mls| async move {
            println!("tester {i} before adding members");
            mls.print_ratchet_tree("");
            // dbg!(mls.export_ratchet_tree());
            Ok::<_, crate::groups::GroupError>(())
        })
        .instrument(tracing::error_span!("g before adding members", i))
        .await
        .unwrap();
        g.add_members_by_inbox_id(&[tester.inbox_id()])
            .instrument(tracing::error_span!("g adding members"))
            .await
            .unwrap();
        g.load_mls_group_with_lock_async(|mls| async move {
            println!("tester {i} after adding members");
            mls.print_ratchet_tree("");
            // dbg!(mls.export_ratchet_tree());
            Ok::<_, crate::groups::GroupError>(())
        })
        .instrument(tracing::error_span!("g after adding members", i))
        .await
        .unwrap();
        let tester_group = tester
            .sync_welcomes()
            .instrument(tracing::error_span!("tester syncing welcomes", i))
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        tester_group
            .sync()
            .instrument(tracing::error_span!("tester group syncing", i))
            .await
            .unwrap();
        tester_group
            .send_message(b"hello", Default::default())
            .instrument(tracing::error_span!("tester sending message", i))
            .await
            .unwrap();
        g.sync()
            .instrument(tracing::error_span!("g syncing", i))
            .await
            .unwrap();
        g.load_mls_group_with_lock_async(|mls| async move {
            println!("tester {i} after sending message");
            mls.print_ratchet_tree("");
            // dbg!(mls.export_ratchet_tree());
            Ok::<_, crate::groups::GroupError>(())
        })
        .instrument(tracing::error_span!("g tester {i} after sending message"))
        .await
        .unwrap();
        for (j, new_installation) in new_installations.into_iter().enumerate() {
            let ni_group = new_installation
                .sync_welcomes()
                .instrument(tracing::error_span!(
                    "tester installation syncing welcomes",
                    i,
                    j
                ))
                .await
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            ni_group
                .sync()
                .instrument(tracing::error_span!(
                    "tester installation group syncing",
                    i,
                    j
                ))
                .await
                .unwrap();
            ni_group
                .send_message(b"hello", Default::default())
                .instrument(tracing::error_span!(
                    "tester installation sending message",
                    i,
                    j
                ))
                .await
                .unwrap();
            ni_group
                .sync()
                .instrument(tracing::error_span!(
                    "tester installation group syncing",
                    i,
                    j
                ))
                .await
                .unwrap();
            g.sync()
                .instrument(tracing::error_span!(
                    "g syncing tester installation after sending message",
                    i,
                    j
                ))
                .await
                .unwrap();
            g.load_mls_group_with_lock_async(|mls| async move {
                println!("tester {i} installation {j} after sending message");
                mls.print_ratchet_tree("");
                // dbg!(mls.export_ratchet_tree());
                Ok::<_, crate::groups::GroupError>(())
            })
            .instrument(tracing::error_span!(
                "g loading tester installation after sending message",
                i,
                j
            ))
            .await
            .unwrap();
            installations.push(new_installation);
        }
        testers.push(tester);
    }
}

#[xmtp_common::test]
async fn test_initial_commit_sizes() {
    let installation_count = std::env::var("XMTP_MLSTEST_INSTALLATIONS")
        .unwrap_or("0".to_string())
        .parse::<usize>()
        .unwrap();
    let tester_count = std::env::var("XMTP_MLSTEST_TESTERS")
        .unwrap_or("0".to_string())
        .parse::<usize>()
        .unwrap();
    tester!(alix);
    let g = alix.create_group(None, None).unwrap();

    let mut testers = vec![];
    let mut installations = vec![];
    for _ in 0..tester_count {
        tester!(tester);
        for _ in 0..installation_count {
            installations.push(tester.new_installation().await);
        }
        testers.push(tester);
    }
    g.add_members_by_inbox_id(
        testers
            .iter()
            .map(|t| t.inbox_id())
            .collect::<Vec<_>>()
            .as_slice(),
    )
    .await
    .unwrap();
    for tester in testers.iter().chain(&installations) {
        let group = tester
            .sync_welcomes()
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        group.sync().await.unwrap();
        group
            .send_message(b"hello", Default::default())
            .await
            .unwrap();
        g.sync().await.unwrap();
    }
    g.sync().await.unwrap();
    tester!(last);
    g.add_members_by_inbox_id(&[last.inbox_id()]).await.unwrap();
}

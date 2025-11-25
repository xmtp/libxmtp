use tls_codec::Serialize;
use tracing::Instrument;

use crate::{context::XmtpSharedContext, tester};

#[xmtp_common::test]
async fn test_commit_size_one_at_a_time() {
    tester!(alix);
    let g = alix.create_group(None, None).unwrap();

    let signer_fn = || alix.identity().installation_keys.clone();
    let g_provider_fn = || alix.context.mls_provider();
    let mut testers = vec![];
    let mut installations = vec![];
    for _ in 0..10 {
        tester!(tester);
        g.add_members_by_inbox_id(&[tester.inbox_id()])
            .await
            .unwrap();
        for _ in 0..9 {
            let new_installation = tester.new_installation().await;
            let key_package = new_installation
                .identity()
                .new_key_package(&new_installation.client.context.mls_provider(), true)
                .unwrap();
            let storage = g.context.mls_storage();
            let signer = signer_fn();
            let (proposal, commit) = g
                .load_mls_group_with_lock_async(|mut group| async move {
                    crate::groups::mls_sync::with_rollback(
                        storage,
                        &mut group,
                        |group, provider| {
                            dbg!(group.pending_commit());
                            let proposal = group
                                .propose_add_member(provider, &signer, &key_package.key_package)
                                .inspect_err(|e| tracing::error!(%e, "Error proposing add member"))
                                .map_err(|e| crate::groups::GroupError::Any(e.into()))?;
                            let commit = group
                                .commit_to_pending_proposals(provider, &signer)
                                .map_err(|e| crate::groups::GroupError::Any(e.into()))?;
                            Ok::<_, crate::groups::GroupError>((proposal, commit))
                        },
                    )
                    // group
                    //     .propose_add_member(&g_provider, &signer, &key_package.key_package)
                    //     .map_err(|e| crate::groups::GroupError::Any(e.into()))
                })
                .await
                .unwrap();
            tracing::warn!(
                "proposal: {}, proposal_hash: {}, commit: {}, staged_commit: {}, welcome: {}",
                proposal.0.tls_serialize_detached().unwrap().len(),
                proposal.1.tls_serialize_detached().unwrap().len(),
                commit.0.tls_serialize_detached().unwrap().len(),
                commit.1.tls_serialize_detached().unwrap().len(),
                commit.2.unwrap().tls_serialize_detached().unwrap().len(),
            );
            // let (data, _) = crate::groups::mls_ext::wrap_welcome(
            //     &welcome.unwrap().tls_serialize_detached().unwrap(),
            //     &[],
            //     &key_package.pq_pub_key.as_ref().unwrap().clone(),
            //     crate::groups::mls_ext::WrapperAlgorithm::XWingMLKEM768Draft6,
            // )
            // .unwrap();
            // let new_installation_group = crate::groups::welcome_sync::WelcomeService::new(new_installation.context.clone())
            //     .process_new_welcome(
            //         &xmtp_proto::types::WelcomeMessage::builder()
            //             .cursor(xmtp_proto::types::Cursor::default())
            //             .created_ns(chrono::Utc::now())
            //             .variant(xmtp_proto::types::WelcomeMessageType::V1(
            //                 xmtp_proto::types::WelcomeMessageV1::builder()
            //                     .installation_key(new_installation.identity().installation_id())
            //                     .data(data)
            //                     .hpke_public_key(key_package.pq_pub_key.as_ref().unwrap().clone())
            //                     .wrapper_algorithm(
            //                         xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperAlgorithm::XwingMlkem768Draft6,
            //                     )
            //                     .build().unwrap(),
            //             ))
            //             .build()
            //             .unwrap(),
            //         false,
            //         crate::groups::InitialMembershipValidator::new(&new_installation.context),
            //     )
            //     .await
            //     .unwrap()
            //     .unwrap();

            // let messages = g
            //     .prepare_group_messages(vec![(
            //         out.tls_serialize_detached().unwrap().as_slice(),
            //         false,
            //     )])
            //     .unwrap();
            // g.context.api().send_group_messages(messages).await.unwrap();
            // g.sync().await.unwrap();

            // let g_provider = g_provider_fn();
            // let signer = signer_fn();
            // let (commit, welcome, group_info) = g
            //     .load_mls_group_with_lock_async(|mut group| async move {
            //         dbg!(&group.pending_proposals().collect::<Vec<_>>().len());
            //         for proposal in group.pending_proposals() {
            //             dbg!(proposal);
            //         }
            //         group
            //             .commit_to_pending_proposals(&g_provider, &signer)
            //             .map_err(|e| crate::groups::GroupError::Any(e.into()))
            //     })
            //     .await
            //     .unwrap();
            g.update_installations().await.unwrap();
            installations.push(new_installation);
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

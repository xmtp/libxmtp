//! Incoming controller tests: proof dependencies.

use super::*;

#[xmtp_common::test(unwrap_try = true)]
async fn stale_dependency_results_cannot_change_new_heads_or_requirements() {
    use xmtp_mls_identity::identity_updates::IdentityDependencyError;

    let alix = EngineTestPeer::new().await;
    for topic in [
        Topic::new_group_message(GroupId::generate()),
        Topic::new_identity_update(hex::decode(alix.inbox_id())?),
        Topic::new_welcome_message(alix.context.installation_id()),
    ] {
        let mut controller = controller(alix.context.clone());
        let key = topic_key(&topic)?;
        let after = controller.context.db().topic_progress(&key)?.received;
        let old = Cursor(after.0 + 10);
        let current = Cursor(after.0 + 20);
        controller.admit_received_batch(OrderedEnvelopeBatch {
            topic: topic.clone(),
            after,
            envelopes: [old, current]
                .into_iter()
                .map(|cursor| wire::ServerEnvelope {
                    meta: Some(meta(&topic, cursor.0)),
                    envelope: Some(wire::ClientEnvelope::default()),
                })
                .map(Into::into)
                .collect(),
        })?;
        controller.topics.entry(topic.clone()).or_default();
        controller
            .context
            .db()
            .complete_pending_envelope(&key, old)?;
        let requirement = IdentityRequirement {
            inbox_id: alix.inbox_id().to_string(),
            sequence_id: 1,
        };
        let next_requirement = IdentityRequirement {
            sequence_id: 2,
            ..requirement.clone()
        };
        let parent = |cursor| match topic.kind() {
            TopicKind::GroupMessagesV1 => DependencyParent::GroupHead(topic.clone(), cursor),
            TopicKind::IdentityUpdatesV1 => DependencyParent::IdentityHead(topic.clone(), cursor),
            _ => DependencyParent::Welcome(cursor),
        };
        for result in [
            Ok(()),
            Err(IdentityDependencyError::MissingReference(
                requirement.clone(),
            )),
            Err(IdentityDependencyError::InvalidSequence(0)),
        ] {
            // A Welcome can change its requirement without changing its cursor.
            let old_parent = if topic.kind() == TopicKind::WelcomeMessagesV1 {
                parent(current)
            } else {
                parent(old)
            };
            controller
                .dependency_registry
                .attach(old_parent, identity_key(&requirement));
            controller
                .dependency_registry
                .attach(parent(current), identity_key(&next_requirement));
            controller.dependency_finished(DependencyResult::Identity(
                BTreeSet::from([requirement.clone()]),
                result,
            ));
            let pending = controller
                .context
                .db()
                .pending_envelope(&key, current)?
                .unwrap();
            assert!(!pending.blocked);
            assert_eq!(pending.retry_at_ns, 0);
            assert!(pending.error_code.is_none());
            assert!(controller.dependency_registry.contains(&parent(current)));
            let state = controller.topics.get(&topic).unwrap();
            assert!(state.error.is_none());
            assert!(state.processing.missing_reference.is_none());
        }
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn passive_prefixes_do_not_limit_shared_identity_requests() {
    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    let limit = controller
        .context
        .incoming_runtime()
        .policy()
        .max_dependency_requests;
    for index in 0..limit {
        controller.dependency_registry.attach(
            DependencyParent::Welcome(Cursor(index as u64 + 10)),
            DependencyKey::GroupPrefix(GroupId::generate(), Cursor(1)),
        );
    }
    let requirement = IdentityRequirement {
        inbox_id: alix.inbox_id().to_string(),
        sequence_id: 0,
    };
    let group =
        DependencyParent::GroupHead(Topic::new_group_message(GroupId::generate()), Cursor(1));
    let welcome = DependencyParent::Welcome(Cursor(limit as u64 + 10));
    for parent in [group.clone(), welcome.clone()] {
        controller
            .dependency_registry
            .attach(parent, identity_key(&requirement));
    }
    controller.start_dependencies();
    assert_eq!(controller.dependencies.len(), 1);
    assert_eq!(controller.dependency_registry.prefixes().count(), limit);
    let result = controller.dependencies.next().await.unwrap();
    assert!(matches!(
        result,
        DependencyResult::Identity(
            _,
            Err(xmtp_mls_identity::identity_updates::IdentityDependencyError::InvalidSequence(0))
        )
    ));
    assert_eq!(
        controller
            .dependency_registry
            .finish(&identity_key(&requirement)),
        HashSet::from([group, welcome])
    );
}

/// One attempt names every member proof the joiner lacks, one request fetches
/// them, and the next attempt joins. An inline Welcome resolves them through
/// the controller; a pointer resolves them inside its Welcome dependency.
// verifies: JOIN-051
#[rstest::rstest]
#[case::inline(false)]
#[case::pointer(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn a_welcome_fetches_missing_member_proofs_in_one_request(#[case] pointer: bool) {
    use crate::groups::welcome_sync::pending_welcome_for_test;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let eve = EngineTestPeer::new().await;
    let others = [caro.inbox_id(), dave.inbox_id(), eve.inbox_id()];
    let group = alix.create_group(None, None).unwrap();
    if pointer {
        // More than two recipient installations receive a welcome pointer.
        group
            .add_members(&[
                caro.inbox_id(),
                dave.inbox_id(),
                eve.inbox_id(),
                bo.inbox_id(),
            ])
            .await
            .unwrap();
    } else {
        group.add_members(&others).await.unwrap();
        group.add_members(&[bo.inbox_id()]).await.unwrap();
    }
    let welcome = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        matches!(
            welcome.variant,
            xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
        ),
        pointer
    );
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    let mut controller = controller(bo.context.clone());
    controller
        .read_queue
        .push_back(Topic::new_welcome_message(bo.context.installation_id()));
    controller.welcome_blocked_scan = None;
    controller.process_ready(&mut false).await;
    assert_eq!(controller.dependencies.len(), 1);

    let queries = bo.api_stats().query.get_count();
    let result = controller.dependencies.next().await.unwrap();
    if let DependencyResult::Identity(requirements, _) = &result {
        let inboxes: HashSet<_> = requirements
            .iter()
            .map(|requirement| requirement.inbox_id.as_str())
            .collect();
        assert!(inboxes.contains(alix.inbox_id()));
        assert!(others.iter().all(|inbox_id| inboxes.contains(inbox_id)));
    } else {
        assert!(pointer);
    }
    controller.dependency_finished(result);
    // A pointer also fetches its pointee.
    assert_eq!(
        bo.api_stats().query.get_count() - queries,
        1 + usize::from(pointer)
    );
    assert!(
        bo.context
            .db()
            .find_group(&group.group_id)
            .unwrap()
            .is_some()
    );
    assert!(controller.dependencies.is_empty());
}

/// One attempt names every proof a multi-inbox add lacks, one request fetches
/// them, and the next attempt applies the commit.
// verifies: GMOD-013
#[xmtp_common::test(unwrap_try = true)]
async fn a_commit_fetches_missing_proofs_for_added_inboxes_in_one_request() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let caro = EngineTestPeer::new().await;
    let dave = EngineTestPeer::new().await;
    let eve = EngineTestPeer::new().await;
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.find_groups(GroupQueryArgs::default())?.remove(0);
    bo_group.receive().await?;
    let added = [caro.inbox_id(), dave.inbox_id(), eve.inbox_id()];
    alix_group.add_members(&added).await?;
    let topic = Topic::new_group_message(bo_group.group_id);
    MlsStore::new(bo.context.clone())
        .receive_topics_once(
            std::slice::from_ref(&topic),
            bo.context
                .incoming_runtime()
                .policy()
                .incoming_limits(NetworkEntityKind::Group),
        )
        .await?;
    let mut controller = controller(bo.context.clone());
    controller.read_queue.push_back(topic);
    // Earlier heads from the add need no proofs. The commit then waits on one batch.
    while controller.process_ready(&mut false).await {}
    assert_eq!(controller.dependencies.len(), 1);

    let queries = bo.api_stats().query.get_count();
    let result = controller.dependencies.next().await.unwrap();
    let DependencyResult::Identity(requirements, Ok(())) = &result else {
        panic!("the commit must wait on one resolved identity batch");
    };
    let inboxes: HashSet<_> = requirements
        .iter()
        .map(|requirement| requirement.inbox_id.as_str())
        .collect();
    assert_eq!(inboxes, HashSet::from(added));
    controller.dependency_finished(result);
    assert_eq!(bo.api_stats().query.get_count() - queries, 1);
    assert!(controller.process_ready(&mut false).await);
    assert!(controller.dependencies.is_empty());
    assert_eq!(bo_group.members().await?.len(), 5);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_welcome_failure_keeps_independent_pending_parents_runnable() {
    use xmtp_mls_identity::identity_updates::IdentityDependencyError;

    let alix = EngineTestPeer::new().await;
    let mut controller = controller(alix.context.clone());
    let topic = Topic::new_welcome_message(controller.context.installation_id());
    let key = topic_key(&topic)?;
    let deadline = Instant::now()
        + controller
            .context
            .incoming_runtime()
            .policy()
            .barrier_timeout;
    add_barrier_scope(&mut controller, 1, &topic, Cursor(30), deadline);
    controller.reconcile()?;
    controller.registered([(topic.clone(), Cursor(30))].into());
    controller.incoming(Some(Ok(IncomingEvent::OrderedBatch(
        OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: [10, 20]
                .into_iter()
                .map(|sequence| wire::ServerEnvelope {
                    meta: Some(meta(&topic, sequence)),
                    envelope: Some(wire::ClientEnvelope::default()),
                })
                .map(Into::into)
                .collect(),
        },
    ))));
    let requirement = IdentityRequirement {
        inbox_id: controller.context.inbox_id().to_string(),
        sequence_id: 0,
    };
    controller.dependency_registry.attach(
        DependencyParent::Welcome(Cursor(10)),
        identity_key(&requirement),
    );
    controller.dependency_finished(DependencyResult::Identity(
        BTreeSet::from([requirement]),
        Err(IdentityDependencyError::InvalidSequence(0)),
    ));
    controller.refresh_statuses();
    let status = controller.state.statuses.lock()[&1].topics[0].clone();
    assert_eq!(status.unresolved_welcomes, 2);
    assert_eq!(status.processing, IncomingProcessing::Pending);
    assert!(status.error.is_some_and(|error| !error.is_retryable()));

    controller.source_error(NetworkError::new(xmtp_api::ApiError::InvalidResponse(
        "cursor order",
    )));
    controller.refresh_statuses();
    let snapshot = controller.state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.connection, IncomingConnection::Failed);
    assert_eq!(snapshot.processing, IncomingProcessing::Pending);
    assert_eq!(snapshot.topics[0].processing, IncomingProcessing::Pending);

    controller
        .context
        .db()
        .complete_pending_envelope(&key, Cursor(20))?;
    controller.refresh_statuses();
    let snapshot = controller.state.statuses.lock()[&1].clone();
    assert_eq!(snapshot.processing, IncomingProcessing::Blocked);
    assert_eq!(snapshot.topics[0].unresolved_welcomes, 1);
    assert_eq!(snapshot.topics[0].received, Cursor(20));
}

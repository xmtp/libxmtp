//! Controller reconcile: matching the open streams to the interesting topics, and
//! rechecking retired ones.

use super::*;

impl<C: XmtpSharedContext + 'static> Controller<C> {
    pub(super) fn reconcile(&mut self) -> Result<(), IncomingError> {
        self.pending_topics = None;
        let missing_group_topics = self.missing_group_topics.clone();
        // Read the generation once, before any read it gates. A write that
        // commits after this read changes the generation the next pass sees.
        let generation = self
            .context
            .db()
            .group_set_generation()
            .map_err(|error| IncomingError::Storage(error.into()))?;
        // Only a Welcome activates a retired group, and storing its group
        // (insert_or_replace_group) always changes the generation, so an
        // unchanged generation leaves every retired group inactive.
        if self.retired_gate.changed(generation) {
            self.recheck_retired()?;
            self.retired_gate.record(generation);
        }
        let all_groups = self
            .scopes
            .values()
            .any(|scope| matches!(scope.scope, ScopeKind::AllGroups(_)));
        if all_groups && self.discovery_gate.changed(generation) {
            self.discovered_groups = self.discover_all_groups()?;
            self.discovery_gate.record(generation);
        }
        let sync_groups = if self
            .scopes
            .values()
            .any(|scope| matches!(scope.scope, ScopeKind::DeviceSyncGroups))
        {
            self.context
                .db()
                .all_sync_groups()
                .map_err(|error| IncomingError::Storage(error.into()))?
                .into_iter()
                .map(|group| Topic::new_group_message(group.id))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };
        for scope in self.scopes.values_mut() {
            // An all-groups scope keeps its topics while the discovered set is
            // the one they came from.
            if let ScopeKind::AllGroups(group_set) = &scope.scope
                && !group_set.changed(generation)
            {
                continue;
            }
            let selected_scope = if let ScopeKind::Groups(groups) = &scope.scope {
                Some(
                    self.context
                        .db()
                        .resolve_group_scope(groups)
                        .map_err(IncomingError::Storage)?,
                )
            } else {
                None
            };
            if let Some(selected_scope) = &selected_scope {
                for group_id in &selected_scope.group_ids {
                    let topic = Topic::new_group_message(group_id);
                    if !scope.topics.contains(&topic) {
                        // An empty pending queue cannot report an already inactive group.
                        let group = match MlsGroup::load_by_id(&self.context, group_id) {
                            Ok(group) => group,
                            Err(xmtp_mls_core::mls_store::MlsStoreError::NotFound(_)) => {
                                self.missing_group_topics.insert(topic);
                                continue;
                            }
                            Err(error) => return Err(error.into()),
                        };
                        let active = group.is_active()?;
                        self.missing_group_topics.remove(&topic);
                        if !active {
                            self.topics.entry(topic).or_default().processing.retired = true;
                        }
                    }
                }
            }
            let topics: HashSet<_> = match &scope.scope {
                ScopeKind::Topics(topics) => topics.iter().cloned().collect(),
                ScopeKind::Groups(_) => selected_scope
                    .iter()
                    .flat_map(|scope| scope.group_ids.iter())
                    .map(Topic::new_group_message)
                    .collect(),
                ScopeKind::Barrier { .. } => scope.targets.keys().cloned().collect(),
                ScopeKind::AllGroups(_) => self
                    .discovered_groups
                    .iter()
                    .cloned()
                    .chain(std::iter::once(Topic::new_welcome_message(
                        self.context.installation_id(),
                    )))
                    .collect(),
                ScopeKind::DeviceSyncGroups => sync_groups
                    .iter()
                    .cloned()
                    .chain(std::iter::once(Topic::new_welcome_message(
                        self.context.installation_id(),
                    )))
                    .collect(),
            };
            // Check state when a scope selects a group. Only an import creates
            // a Restored row. Keep missing rows for later checks. A lookup error
            // fails the pass before this scope selects its new topics.
            // implements: PROC-051
            if !matches!(scope.scope, ScopeKind::Groups(_) | ScopeKind::AllGroups(_)) {
                for topic in topics.difference(&scope.topics) {
                    if topic.kind() != TopicKind::GroupMessagesV1 {
                        continue;
                    }
                    let Ok(group_id) = GroupId::try_from(topic.identifier()) else {
                        continue;
                    };
                    match stored_membership(&self.context, &group_id)? {
                        Some(state) => {
                            self.missing_group_topics.remove(topic);
                            if state == GroupMembershipState::Restored {
                                self.topics
                                    .entry(topic.clone())
                                    .or_default()
                                    .processing
                                    .retired = true;
                            }
                        }
                        None => {
                            self.missing_group_topics.insert(topic.clone());
                        }
                    }
                }
            }
            scope.topics = topics;
            scope
                .targets
                .retain(|topic, _| scope.topics.contains(topic));
            if let ScopeKind::AllGroups(group_set) = &mut scope.scope {
                group_set.record(generation);
            }
        }
        let scopes = &self.scopes;
        self.missing_group_topics
            .retain(|topic| scopes.values().any(|scope| scope.topics.contains(topic)));
        // Stored groups need no repeat lookup. A missing row can be imported
        // while its topic is selected. Check it before adding network interest.
        // implements: PROC-051
        for topic in missing_group_topics {
            if !self.missing_group_topics.contains(&topic) {
                continue;
            }
            let group_id = GroupId::try_from(topic.identifier())
                .map_err(|_| IncomingError::UnsupportedTopic)?;
            if let Some(state) = stored_membership(&self.context, &group_id)? {
                if state == GroupMembershipState::Restored {
                    self.topics
                        .entry(topic.clone())
                        .or_default()
                        .processing
                        .retired = true;
                }
                self.missing_group_topics.remove(&topic);
            }
        }
        self.extra_topics = self
            .dependency_registry
            .prefixes()
            .map(|(_, group, _)| Topic::new_group_message(group))
            .collect();
        let watched: HashSet<_> = self
            .dependency_registry
            .prefixes()
            .map(|(welcome, _, _)| welcome)
            .collect();
        self.prefix_holds
            .retain(|welcome, _| watched.contains(welcome));
        if self.scopes.values().any(|scope| match &scope.scope {
            ScopeKind::Groups(requested) => requested
                .iter()
                .any(|id| self.is_retired(&Topic::new_group_message(id))),
            _ => false,
        }) {
            // Only explicitly requested inactive groups add Welcome recovery.
            // The dependency does not add a fixed target or a delivery scope.
            self.extra_topics
                .insert(Topic::new_welcome_message(self.context.installation_id()));
        }
        // The retired check at the top of this function would otherwise run on every
        // pass for a topic that no operation selects. Selecting it again repeats the
        // scope entry checks.
        let (scopes, extra_topics) = (&self.scopes, &self.extra_topics);
        self.topics.retain(|topic, state| {
            !state.processing.retired
                || extra_topics.contains(topic)
                || scopes.values().any(|scope| scope.topics.contains(topic))
        });
        let interested = self.interested();
        self.dependency_registry
            .retain_parents(|parent| match parent {
                DependencyParent::GroupHead(topic, _)
                | DependencyParent::IdentityHead(topic, _) => interested.contains(topic),
                DependencyParent::Welcome(_) => true,
            });
        for (topic, state) in &mut self.topics {
            if !interested.contains(topic) {
                state.processing.missing_reference = None;
                state.error = None;
                state.receipt.last_read = None;
                state.receipt.received_floor = None;
            }
        }
        self.callbacks
            .retain(|group, _| interested.contains(&Topic::new_group_message(*group)));
        merge_read_queue(&mut self.read_queue, &interested, Topic::cloned_vec);
        let mut pending_topics = HashSet::new();
        // A drained topic can rejoin reception. Other kinds keep their own capacity.
        for kind in [
            NetworkEntityKind::Group,
            NetworkEntityKind::Welcome,
            NetworkEntityKind::Identity,
        ] {
            let usage = self.context.db().pending_topic_usage(kind)?;
            if kind != NetworkEntityKind::Welcome {
                pending_topics.extend(usage.iter().map(|usage| StreamTopic {
                    entity_id: usage.entity_id.clone(),
                    kind,
                }));
            }
            let limits = self
                .context
                .incoming_runtime()
                .policy()
                .incoming_limits(kind);
            let ready_pool = kind_pool_usage(&usage, false);
            let held_pool = kind_pool_usage(&usage, true);
            for (topic, state) in &mut self.topics {
                if !state.receipt.paused {
                    continue;
                }
                let Ok(stream_topic) = topic_key(topic) else {
                    continue;
                };
                if stream_topic.kind != kind {
                    continue;
                }
                let own = usage
                    .iter()
                    .find(|usage| usage.entity_id == stream_topic.entity_id);
                // A topic waits on the pool its admission is charged to.
                let held = own.is_some_and(|usage| usage.charges_held(kind));
                let (kind_rows, kind_bytes) = if held { held_pool } else { ready_pool };
                // Leave room for the next bounded batch before reception resumes.
                state.receipt.paused = kind_rows > limits.kind.rows / 2
                    || kind_bytes > limits.kind.bytes / 2
                    || own.is_some_and(|usage| {
                        let (rows, bytes) = usage.pool(held);
                        rows > limits.topic.rows / 2 || bytes > limits.topic.bytes / 2
                    });
            }
        }
        self.pending_topics = Some(pending_topics);
        Ok(())
    }

    /// Check each retired topic once, and return each whose group is active
    /// again to processing.
    fn recheck_retired(&mut self) -> Result<(), IncomingError> {
        let retired: Vec<_> = self
            .topics
            .iter()
            .filter(|(_, state)| state.processing.retired)
            .map(|(topic, _)| topic.clone())
            .collect();
        for topic in retired {
            #[cfg(test)]
            {
                self.retired_checks += 1;
            }
            let group_id = topic
                .identifier()
                .try_into()
                .map_err(|_| IncomingError::UnsupportedTopic)?;
            if !MlsGroup::load_by_id(&self.context, &group_id)?.is_active()? {
                continue;
            }
            self.dependency_registry
                .retain_parents(|parent| match parent {
                    DependencyParent::GroupHead(current, _) => current != &topic,
                    _ => true,
                });
            let state = self.topics.entry(topic).or_default();
            state.processing.retired = false;
            state.processing.missing_reference = None;
            state.error = None;
            state.receipt.last_read = None;
        }
        Ok(())
    }

    /// The topic of every stored group, whatever its consent, that an
    /// all-groups scope receives.
    // implements: PROC-046
    fn discover_all_groups(&mut self) -> Result<HashSet<Topic>, IncomingError> {
        #[cfg(test)]
        {
            self.discovery_reads += 1;
        }
        Ok(self
            .context
            .db()
            // A Restored archive placeholder is inert until a Welcome activates it.
            // implements: PROC-051
            .stored_group_refs(&GroupScope::every_group().without_restored())
            .map_err(|error| IncomingError::Storage(error.into()))?
            .into_iter()
            .map(|group| Topic::new_group_message(group.id))
            .collect())
    }

    pub(super) fn interested(&self) -> HashSet<Topic> {
        self.scopes
            .values()
            .flat_map(|scope| scope.topics.iter())
            .chain(self.extra_topics.iter())
            // A request that names a topic the backend does not route fails as
            // a whole. Such a group topic carries a 16-byte id, and only an
            // import stores one, as a Restored placeholder. So a selected
            // 16-byte id, stored or not, never gets network interest.
            // implements: PROC-051
            .filter(|topic| {
                !self.is_retired(topic) && topic_key(topic).is_ok() && Topic::parse(topic).is_ok()
            })
            .cloned()
            .collect()
    }
}

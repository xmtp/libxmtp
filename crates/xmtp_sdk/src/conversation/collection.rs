#[derive(uniffi::Object)]
pub struct Conversations {
    pub(crate) client: Arc<CoreClient>,
    pub(crate) client_key: u64,
}

#[derive(Clone, uniffi::Enum)]
pub enum Conversation {
    Group { group: Arc<Group> },
    Dm { dm: Arc<Dm> },
}

impl Conversation {
    pub(crate) async fn from_core(
        group: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
    ) -> Result<Option<Self>, XmtpError> {
        Ok(match group.conversation_type {
            ConversationType::Group => Some(Self::Group {
                group: Arc::new(Group::from_core(group, client_key).await?),
            }),
            ConversationType::Dm => Some(Self::Dm {
                dm: Arc::new(Dm::from_core(group, client_key).await?),
            }),
            ConversationType::Sync | ConversationType::Oneshot => None,
        })
    }

    fn from_preloaded(
        group: MlsGroup<MlsContext>,
        client_key: u64,
        identity: ConversationIdentity,
        metadata: xmtp_mls::mls_common::group_metadata::GroupMetadata,
    ) -> Result<Option<Self>, XmtpError> {
        Ok(match group.conversation_type {
            ConversationType::Group => Some(Self::Group {
                group: Arc::new(Group {
                    inner: group,
                    client_key,
                    identity,
                    #[cfg(all(test, not(target_arch = "wasm32")))]
                    state_counts: Arc::new(parking_lot::Mutex::new((0, 0, 0))),
                    #[cfg(all(test, not(target_arch = "wasm32")))]
                    history_query_count: Arc::new(parking_lot::Mutex::new(0)),
                }),
            }),
            ConversationType::Dm => Some(Self::Dm {
                dm: Arc::new(Dm::from_metadata(group, client_key, identity, metadata)?),
            }),
            ConversationType::Sync | ConversationType::Oneshot => None,
        })
    }
}

pub(crate) async fn list_local(
    client: Arc<crate::client::CoreClient>,
    client_key: u64,
    args: GroupQueryArgs,
) -> Result<Vec<Conversation>, XmtpError> {
    use xmtp_mls::mls_common::app_data::component_source::read_group_metadata_from_extensions;
    use xmtp_mls::mls_common::group_metadata::GroupMetadata;
    use xmtp_proto::xmtp::mls::message_contents::GroupMetadataV1;

    let items = client
        .list_conversations(args)
        .map_err(XmtpError::from_core)?;
    let ids = items
        .iter()
        .map(|item| item.group.group_id)
        .collect::<Vec<_>>();
    let contexts = client
        .context
        .mls_storage()
        .read_group_contexts(&ids)
        .map_err(XmtpError::from_core)?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let context = contexts
            .get(&item.group.group_id)
            .ok_or_else(|| XmtpError::unknown("conversation group context is missing"))?;
        let seed = read_group_metadata_from_extensions(context.extensions())
            .map_err(XmtpError::from_core)?
            .ok_or_else(|| XmtpError::unknown("conversation metadata is missing"))?;
        let metadata = GroupMetadata::try_from(GroupMetadataV1 {
            conversation_type: seed.conversation_type,
            creator_inbox_id: seed.creator_inbox_id,
            creator_account_address: String::new(),
            dm_members: seed.dm_members,
            oneshot_message: seed.oneshot,
        })
        .map_err(XmtpError::from_core)?;
        let identity = ConversationIdentity::from_metadata(
            item.added_by_inbox_id,
            &metadata,
            client.inbox_id(),
            item.membership_state,
        );
        if let Some(conversation) =
            Conversation::from_preloaded(item.group, client_key, identity, metadata)?
        {
            result.push(conversation);
        }
    }
    Ok(result)
}

#[xmtp_macro::sdk_export]
impl Conversations {
    /// Capture history with kind and consent selection. A replay `from` cursor
    /// is invalid because this call captures a new atomic replay boundary.
    #[uniffi::method(default(options = None))]
    pub async fn message_history_snapshot(
        &self,
        limit: u32,
        options: Option<crate::MessageReaderOptions>,
    ) -> Result<crate::MessageHistorySnapshot, XmtpError> {
        use xmtp_mls::subscriptions::local_delivery::{DeliveryScope, LocalDeliveryFilter};
        let context = self.client.context.clone();
        let client_key = self.client_key;
        let options = options.unwrap_or_default();
        if options.from.is_some() {
            return Err(XmtpError::invalid_argument(
                "history snapshot does not take a replay cursor",
            ));
        }
        on_sdk_worker(self.client.context.clone(), async move {
            let filter = LocalDeliveryFilter {
                conversation_type: options.conversation_kind.map(|kind| match kind {
                    crate::ConversationKind::Group => ConversationType::Group,
                    crate::ConversationKind::Dm => ConversationType::Dm,
                }),
                consent_states: options
                    .consent_states
                    .map(|states| states.into_iter().map(Into::into).collect()),
            };
            crate::delivery::history_snapshot(
                &context,
                &DeliveryScope::All,
                &filter,
                limit,
                client_key,
            )
        })
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn message_reader(
        &self,
        options: Option<crate::MessageReaderOptions>,
    ) -> Result<Arc<MessageReader>, XmtpError> {
        use xmtp_mls::subscriptions::local_delivery::{DeliveryScope, LocalDeliveryFilter};
        let context = self.client.context.clone();
        let client_key = self.client_key;
        let options = options.unwrap_or_default();
        on_sdk_worker(self.client.context.clone(), async move {
            let filter = LocalDeliveryFilter {
                conversation_type: options.conversation_kind.map(|kind| match kind {
                    crate::ConversationKind::Group => ConversationType::Group,
                    crate::ConversationKind::Dm => ConversationType::Dm,
                }),
                consent_states: options
                    .consent_states
                    .map(|states| states.into_iter().map(Into::into).collect()),
            };
            MessageReader::open(
                context,
                DeliveryScope::All,
                filter,
                options.from,
                client_key,
            )
        })
        .await
    }

    pub async fn beginning_delivery_cursor(&self) -> Result<String, XmtpError> {
        use xmtp_db::delivery::{DeliveryCursor, QueryDelivery};
        let context = self.client.context.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let database_id = context.db().stream_database_id().map_err(|error| {
                crate::delivery::delivery_error(
                    xmtp_mls::subscriptions::local_delivery::LocalDeliveryError::Storage(error),
                )
            })?;
            Ok(crate::delivery::cursor::encode(DeliveryCursor {
                database_id,
                delivery_sequence: 0,
            }))
        })
        .await
    }

    pub async fn conversation_reader(
        &self,
        options: Option<crate::ConversationReaderOptions>,
    ) -> Result<Arc<ConversationReader>, XmtpError> {
        let context = self.client.context.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            ConversationReader::open(context, options.unwrap_or_default(), client_key).await
        })
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn create_group_optimistic(
        &self,
        options: Option<CreateGroupOptions>,
    ) -> Result<Arc<Group>, XmtpError> {
        let (permissions, metadata) = options.unwrap_or_default().into_core()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = client
                .create_group(permissions, Some(metadata))
                .map_err(XmtpError::from_core)?;
            Ok(Arc::new(Group::from_core(group, client_key).await?))
        })
        .await
    }

    pub async fn get_dm_by_inbox_id(&self, peer: InboxId) -> Result<Option<Arc<Dm>>, XmtpError> {
        let peer = peer.into_checked()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let members = xmtp_mls::mls_common::group_metadata::DmMembers {
                member_one_inbox_id: client.inbox_id(),
                member_two_inbox_id: peer.as_str(),
            };
            let Some(stored) = client
                .context
                .db()
                .find_active_dm_group(&members)
                .map_err(XmtpError::from_core)?
            else {
                return Ok(None);
            };
            let group = client.group(&stored.id).map_err(XmtpError::from_core)?;
            Ok(Some(Arc::new(Dm::from_core(group, client_key).await?)))
        })
        .await
    }

    pub async fn get_dm_by_identity(
        &self,
        identity: PublicIdentity,
    ) -> Result<Option<Arc<Dm>>, XmtpError> {
        let client = self.client.clone();
        let identifier = identity.to_core()?;
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let Some(peer) = client
                .find_inbox_id_from_identifier(&client.context.db(), identifier)
                .await
                .map_err(XmtpError::from_client)?
            else {
                return Ok(None);
            };
            let members = xmtp_mls::mls_common::group_metadata::DmMembers {
                member_one_inbox_id: client.inbox_id(),
                member_two_inbox_id: peer.as_str(),
            };
            let Some(stored) = client
                .context
                .db()
                .find_active_dm_group(&members)
                .map_err(XmtpError::from_core)?
            else {
                return Ok(None);
            };
            let group = client.group(&stored.id).map_err(XmtpError::from_core)?;
            Ok(Some(Arc::new(Dm::from_core(group, client_key).await?)))
        })
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn create_group(
        &self,
        members: Vec<InboxId>,
        options: Option<CreateGroupOptions>,
    ) -> Result<Arc<Group>, XmtpError> {
        let members = members
            .into_iter()
            .map(InboxId::into_checked)
            .collect::<Result<Vec<_>, _>>()?;
        let (permissions, metadata) = options.unwrap_or_default().into_core()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(
            self.client.context.clone(),
            Box::pin(async move {
                let group = client
                    .create_group_with_members(&members, permissions, Some(metadata))
                    .await
                    .map_err(XmtpError::from_core)?;
                Ok(Arc::new(Group::from_core(group, client_key).await?))
            }),
        )
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn create_dm(
        &self,
        peer: InboxId,
        options: Option<CreateDmOptions>,
    ) -> Result<Arc<Dm>, XmtpError> {
        let peer = peer.into_checked()?;
        let client = self.client.clone();
        let metadata = options.unwrap_or_default().into();
        let client_key = self.client_key;
        on_sdk_worker(
            self.client.context.clone(),
            Box::pin(async move {
                let group = client
                    .find_or_create_dm(peer, Some(metadata))
                    .await
                    .map_err(XmtpError::from_core)?;
                Ok(Arc::new(Dm::from_core(group, client_key).await?))
            }),
        )
        .await
    }

    /// Create a group from account identities. Hosts present this as a
    /// `createGroup` overload or union.
    #[uniffi::method(default(options = None))]
    pub async fn create_group_with_identities(
        &self,
        members: Vec<PublicIdentity>,
        options: Option<CreateGroupOptions>,
    ) -> Result<Arc<Group>, XmtpError> {
        let members = members
            .iter()
            .map(PublicIdentity::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        let (permissions, metadata) = options.unwrap_or_default().into_core()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(
            self.client.context.clone(),
            Box::pin(async move {
                let group = client
                    .create_group_with_identifiers(&members, permissions, Some(metadata))
                    .await
                    .map_err(XmtpError::from_core)?;
                Ok(Arc::new(Group::from_core(group, client_key).await?))
            }),
        )
        .await
    }

    /// Find or create a DM with an account identity. Hosts present this as a
    /// `createDm` overload or union.
    #[uniffi::method(default(options = None))]
    pub async fn create_dm_with_identity(
        &self,
        peer: PublicIdentity,
        options: Option<CreateDmOptions>,
    ) -> Result<Arc<Dm>, XmtpError> {
        let peer = peer.to_core()?;
        let client = self.client.clone();
        let metadata = options.unwrap_or_default().into();
        let client_key = self.client_key;
        on_sdk_worker(
            self.client.context.clone(),
            Box::pin(async move {
                let group = client
                    .find_or_create_dm_by_identity(peer, Some(metadata))
                    .await
                    .map_err(XmtpError::from_core)?;
                Ok(Arc::new(Dm::from_core(group, client_key).await?))
            }),
        )
        .await
    }

    pub async fn get_by_id(&self, id: ConversationId) -> Result<Option<Conversation>, XmtpError> {
        let id: GroupId = id.try_into()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            if client
                .context
                .db()
                .find_group(&id)
                .map_err(XmtpError::from_core)?
                .is_none()
            {
                return Ok(None);
            }
            let group = client.stitched_group(&id).map_err(XmtpError::from_core)?;
            Conversation::from_core(group, client_key).await
        })
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn list(
        &self,
        options: Option<ListConversationsOptions>,
    ) -> Result<Vec<Conversation>, XmtpError> {
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let args: GroupQueryArgs = options.unwrap_or_default().into();
            list_local(client, client_key, args).await
        })
        .await
    }

    pub async fn list_groups(
        &self,
        options: Option<ListConversationsOptions>,
    ) -> Result<Vec<Arc<Group>>, XmtpError> {
        let mut options = options.unwrap_or_default();
        options.kind = Some(crate::ConversationKind::Group);
        Ok(self
            .list(Some(options))
            .await?
            .into_iter()
            .filter_map(|item| match item {
                Conversation::Group { group } => Some(group),
                Conversation::Dm { .. } => None,
            })
            .collect())
    }

    pub async fn list_dms(
        &self,
        options: Option<ListConversationsOptions>,
    ) -> Result<Vec<Arc<Dm>>, XmtpError> {
        let mut options = options.unwrap_or_default();
        options.kind = Some(crate::ConversationKind::Dm);
        Ok(self
            .list(Some(options))
            .await?
            .into_iter()
            .filter_map(|item| match item {
                Conversation::Dm { dm } => Some(dm),
                Conversation::Group { .. } => None,
            })
            .collect())
    }

    pub async fn get_message_by_id(&self, id: MessageId) -> Result<Option<Message>, XmtpError> {
        let bytes = id.to_bytes()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            use xmtp_db::delivery::QueryDelivery;
            let Some(row) = client
                .context
                .db()
                .app_visible_message_row(&bytes, xmtp_common::time::now_ns())
                .map_err(XmtpError::from_core)?
            else {
                return Ok(None);
            };
            let stored = row.stored;
            let enriched = xmtp_mls::messages::enrichment::enrich_messages_with_stored(
                client.context.db(),
                &stored.group_id,
                vec![stored.clone()],
            )
            .map_err(XmtpError::from_core)?;
            let message = if let Some(value) = enriched.into_iter().next() {
                Message::from_enriched(
                    value.stored,
                    value.decoded,
                    value.parent_stored,
                    client_key,
                )?
            } else {
                Message::from_stored(stored, client_key)?
            };
            Ok(Some(message.with_delivery_cursor(row.cursor)))
        })
        .await
    }

    pub async fn delete_message_locally(&self, id: MessageId) -> Result<(), XmtpError> {
        let bytes = id.to_bytes()?;
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client.delete_message(bytes).map_err(XmtpError::from_core)?;
            Ok(())
        })
        .await
    }

    pub async fn delete_message(&self, id: MessageId) -> Result<MessageId, XmtpError> {
        let (stored, group) = self.message_group(&id).await?;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = deletion_group(group, &stored)?;
            let deletion_id = group
                .delete_message(stored.id)
                .map_err(XmtpError::from_core)?;
            MessageId::from_bytes(&deletion_id)
        })
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn react_to_message(
        &self,
        id: MessageId,
        reaction: Reaction,
        options: Option<SendOptions>,
    ) -> Result<MessageId, XmtpError> {
        let (stored, group) = self.message_group(&id).await?;
        on_sdk_worker(self.client.context.clone(), async move {
            Box::pin(async move {
                let reference_inbox_id =
                    InboxId::try_from(stored.sender_inbox_id)?.into_checked()?;
                let content = ReactionCodec::encode(
                    reaction.into_proto(id.into_checked()?, reference_inbox_id),
                )
                .map_err(XmtpError::from_core)?;
                send_encoded(group, content.try_into()?, options.unwrap_or_default()).await
            })
            .await
        })
        .await
    }

    #[uniffi::method(default(options = None))]
    pub async fn reply_to_message(
        &self,
        id: MessageId,
        content: EncodedContent,
        options: Option<SendOptions>,
    ) -> Result<MessageId, XmtpError> {
        require_content_type(&content)?;
        let (stored, group) = self.message_group(&id).await?;
        on_sdk_worker(self.client.context.clone(), async move {
            Box::pin(async move {
                let reply = Reply {
                    reference: id.into_checked()?,
                    reference_inbox_id: Some(stored.sender_inbox_id),
                    content: content.into(),
                };
                let encoded = ReplyCodec::encode(reply).map_err(XmtpError::from_core)?;
                send_encoded(group, encoded.try_into()?, options.unwrap_or_default()).await
            })
            .await
        })
        .await
    }

    pub async fn sync(&self) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client.sync_welcomes().await.map_err(XmtpError::from_core)?;
            Ok(())
        })
        .await
    }

    pub async fn sync_all(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(
            self.client.context.clone(),
            Box::pin(async move {
                client
                    .sync_all_welcomes_and_groups(
                        consent_states.map(|states| states.into_iter().map(Into::into).collect()),
                    )
                    .await
                    .map(Into::into)
                    .map_err(XmtpError::from_core)
            }),
        )
        .await
    }

    pub async fn hmac_keys(&self) -> Result<HashMap<String, Vec<HmacKey>>, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let mut groups = client
                .find_groups(GroupQueryArgs {
                    include_duplicate_dms: true,
                    ..Default::default()
                })
                .map_err(XmtpError::from_core)?;
            let mut entries = HashMap::with_capacity(groups.len());
            for group in groups.drain(..) {
                entries.insert(
                    hex::encode(group.group_id.as_slice()),
                    group
                        .hmac_keys(-1..=1)
                        .map_err(XmtpError::from_core)?
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                );
            }
            Ok(entries)
        })
        .await
    }
}

impl Conversations {
    async fn message_group(
        &self,
        id: &MessageId,
    ) -> Result<
        (
            xmtp_db::group_message::StoredGroupMessage,
            MlsGroup<xmtp_mls::MlsContext>,
        ),
        XmtpError,
    > {
        let client = self.client.clone();
        let bytes = id.to_bytes()?;
        on_sdk_worker(self.client.context.clone(), async move {
            client
                .message_with_group(&bytes)
                .await
                .map_err(XmtpError::from_core)?
                .ok_or_else(|| XmtpError::invalid("message not found"))
        })
        .await
    }
}

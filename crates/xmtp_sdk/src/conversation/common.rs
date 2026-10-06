// One block defines the shared Group and Dm methods.
macro_rules! common_conversation {
    ($name:ident, $state:ty, $map:expr) => {
        #[xmtp_macro::sdk_export]
        impl $name {
            #[sdk(immutable)]
            pub fn id(&self) -> ConversationId {
                self.inner.group_id.into()
            }

            #[sdk(immutable)]
            pub fn created_at(&self) -> Timestamp {
                Timestamp(self.inner.created_at_ns)
            }

            #[sdk(immutable)]
            pub fn topic(&self) -> String {
                xmtp_proto::types::Topic::new_group_message(self.inner.group_id).to_string()
            }

            #[sdk(immutable)]
            pub fn kind(&self) -> crate::ConversationKind {
                match self.inner.conversation_type {
                    ConversationType::Dm => crate::ConversationKind::Dm,
                    _ => crate::ConversationKind::Group,
                }
            }

            #[sdk(immutable)]
            pub fn added_by_inbox_id(&self) -> Option<InboxId> {
                self.identity.added_by_inbox_id.clone()
            }

            #[sdk(immutable)]
            pub fn creator_inbox_id(&self) -> Option<InboxId> {
                self.identity.creator_inbox_id.clone()
            }

            #[sdk(immutable)]
            pub fn is_creator(&self) -> bool {
                self.identity.is_creator
            }

            pub async fn state(&self) -> Result<$state, XmtpError> {
                let group = self.inner.clone();
                #[cfg(all(test, not(target_arch = "wasm32")))]
                let counts = self.state_counts.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    #[cfg(all(test, not(target_arch = "wasm32")))]
                    {
                        let ((state, key_reads), queries, writes) =
                            xmtp_db::count_sql_queries(|| {
                                xmtp_db::sql_key_store::count_kv_reads(|| {
                                    let snapshot =
                                        group.state_snapshot().map_err(XmtpError::from_core)?;
                                    ($map)(snapshot)
                                })
                            });
                        *counts.lock() = (queries.saturating_sub(key_reads), key_reads, writes);
                        return state;
                    }
                    #[cfg(any(not(test), target_arch = "wasm32"))]
                    {
                        let snapshot = group.state_snapshot().map_err(XmtpError::from_core)?;
                        ($map)(snapshot)
                    }
                })
                .await
            }

            pub async fn last_activity_at(
                &self,
                content_types: Option<Vec<ContentTypeId>>,
            ) -> Result<Timestamp, XmtpError> {
                let types = content_types.map(query_content_types).transpose()?;
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .last_activity_ns(types.as_deref())
                        .map(Timestamp)
                        .map_err(XmtpError::from_core)
                })
                .await
            }

            pub async fn update_consent_state(&self, state: ConsentState) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .update_consent_state(state.into())
                        .map_err(XmtpError::from_core)
                })
                .await
            }

            pub async fn sync(&self) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(
                    self.inner.context.clone(),
                    Box::pin(async move {
                        group.sync().await.map_err(XmtpError::from_core)?;
                        Ok(())
                    }),
                )
                .await
            }

            pub async fn members(&self) -> Result<Vec<Member>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .members()
                        .await
                        .map_err(XmtpError::from_core)?
                        .into_iter()
                        .map(Member::try_from)
                        .collect()
                })
                .await
            }

            pub async fn debug_info(&self) -> Result<crate::ConversationDebugInfo, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .debug_info()
                        .await
                        .map(Into::into)
                        .map_err(XmtpError::from_core)
                })
                .await
            }

            pub async fn hmac_keys(&self) -> Result<Vec<HmacKey>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    Ok(group
                        .hmac_keys(-1..=1)
                        .map_err(XmtpError::from_core)?
                        .into_iter()
                        .map(Into::into)
                        .collect())
                })
                .await
            }

            pub async fn last_read_times(&self) -> Result<HashMap<String, Timestamp>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .get_last_read_times()
                        .map_err(XmtpError::from_core)?
                        .into_iter()
                        .map(|(inbox_id, ns)| {
                            Ok((InboxId::try_from(inbox_id)?.into_checked()?, Timestamp(ns)))
                        })
                        .collect()
                })
                .await
            }

            pub async fn update_disappearing_settings(
                &self,
                settings: Option<DisappearingSettings>,
            ) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(
                    self.inner.context.clone(),
                    Box::pin(async move {
                        match settings {
                            Some(settings) => {
                                group
                                    .update_conversation_message_disappearing_settings(
                                        settings.into(),
                                    )
                                    .await
                            }
                            None => {
                                group
                                    .remove_conversation_message_disappearing_settings()
                                    .await
                            }
                        }
                        .map_err(XmtpError::from_core)
                    }),
                )
                .await
            }

            pub async fn set_notifications(
                &self,
                value: NotificationOverride,
            ) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .set_notifications(value.into())
                        .map_err(XmtpError::from_core)
                })
                .await
            }

            pub async fn publish_messages(&self) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(
                    self.inner.context.clone(),
                    Box::pin(async move {
                        group
                            .publish_messages()
                            .await
                            .map_err(XmtpError::from_group_write)
                    }),
                )
                .await
            }

            pub async fn publish_message(&self, id: MessageId) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                let bytes = id.to_bytes()?;
                on_sdk_worker(
                    self.inner.context.clone(),
                    Box::pin(async move {
                        group
                            .publish_stored_message(&bytes)
                            .await
                            .map_err(XmtpError::from_group_write)
                    }),
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn prepare_message(
                &self,
                encoded: EncodedContent,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                let mut options = options.unwrap_or_default();
                options.optimistic = true;
                send_encoded(self.inner.clone(), encoded, options).await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send(
                &self,
                encoded: EncodedContent,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_encoded(self.inner.clone(), encoded, options.unwrap_or_default()).await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_text(
                &self,
                text: String,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(self.inner.clone(), StandardContent::Text(text), options).await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_markdown(
                &self,
                markdown: String,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Markdown(markdown),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_reaction(
                &self,
                reference: MessageId,
                reference_inbox_id: Option<InboxId>,
                reaction: Reaction,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Reaction {
                        reference,
                        reference_inbox_id,
                        reaction,
                    },
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_reply(
                &self,
                reference: MessageId,
                reference_inbox_id: Option<InboxId>,
                content: EncodedContent,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Reply {
                        reference,
                        reference_inbox_id,
                        content,
                    },
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_read_receipt(
                &self,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(self.inner.clone(), StandardContent::ReadReceipt, options).await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_attachment(
                &self,
                attachment: crate::Attachment,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Attachment(attachment),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_remote_attachment(
                &self,
                attachment: crate::RemoteAttachment,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::RemoteAttachment(attachment),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_multi_remote_attachment(
                &self,
                attachment: crate::MultiRemoteAttachment,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::MultiRemoteAttachment(attachment),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_transaction_reference(
                &self,
                reference: crate::TransactionReference,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::TransactionReference(reference),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_wallet_send_calls(
                &self,
                calls: crate::WalletSendCalls,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::WalletSendCalls(calls),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_actions(
                &self,
                actions: crate::Actions,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Actions(actions),
                    options,
                )
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn send_intent(
                &self,
                intent: crate::Intent,
                options: Option<SendOptions>,
            ) -> Result<MessageId, XmtpError> {
                send_standard(self.inner.clone(), StandardContent::Intent(intent), options).await
            }

            #[uniffi::method(default(options = None))]
            pub async fn messages(
                &self,
                options: Option<ListMessagesOptions>,
            ) -> Result<Vec<Message>, XmtpError> {
                let query: MsgQueryArgs = options.unwrap_or_default().try_into()?;
                let group = self.inner.clone();
                let client_key = self.client_key;
                #[cfg(all(test, not(target_arch = "wasm32")))]
                let history_query_count = self.history_query_count.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    let load = || -> Result<Vec<Message>, XmtpError> {
                        let enriched = group
                            .find_messages_v2_with_stored(&query)
                            .map_err(XmtpError::from_core)?;
                        Ok(lift_history_messages(enriched, client_key))
                    };
                    #[cfg(all(test, not(target_arch = "wasm32")))]
                    {
                        let (messages, queries, _) = xmtp_db::count_sql_queries(load);
                        *history_query_count.lock() = queries;
                        messages
                    }
                    #[cfg(any(not(test), target_arch = "wasm32"))]
                    {
                        load()
                    }
                })
                .await
            }

            pub async fn count_messages(
                &self,
                options: Option<ListMessagesOptions>,
            ) -> Result<u64, XmtpError> {
                let query: MsgQueryArgs = options.unwrap_or_default().try_into()?;
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .count_messages(&query)
                        .map(|count| count as u64)
                        .map_err(XmtpError::from_core)
                })
                .await
            }

            pub async fn last_message(&self) -> Result<Option<Message>, XmtpError> {
                Ok(self
                    .messages(Some(ListMessagesOptions {
                        limit: Some(1),
                        direction: Some(crate::MessageOrder::Descending),
                        ..Default::default()
                    }))
                    .await?
                    .into_iter()
                    .next())
            }

            pub async fn delete_message(&self, id: MessageId) -> Result<MessageId, XmtpError> {
                let group = self.inner.clone();
                let bytes = id.to_bytes()?;
                on_sdk_worker(self.inner.context.clone(), async move {
                    let stored = group
                        .context
                        .db()
                        .get_group_message(&bytes)
                        .map_err(XmtpError::from_core)?
                        .ok_or_else(|| XmtpError::invalid("message not found"))?;
                    let group = deletion_group(group, &stored)?;
                    let deletion_id = group.delete_message(bytes).map_err(XmtpError::from_core)?;
                    MessageId::from_bytes(&deletion_id)
                })
                .await
            }

            pub async fn message_history_snapshot(
                &self,
                limit: u32,
            ) -> Result<crate::MessageHistorySnapshot, XmtpError> {
                let context = self.inner.context.clone();
                let group_id = self.inner.group_id;
                let client_key = self.client_key;
                on_sdk_worker(self.inner.context.clone(), async move {
                    crate::delivery::history_snapshot(
                        &context,
                        &xmtp_mls::subscriptions::local_delivery::DeliveryScope::Groups(vec![
                            group_id,
                        ]),
                        &Default::default(),
                        limit,
                        client_key,
                    )
                })
                .await
            }

            #[uniffi::method(default(options = None))]
            pub async fn message_reader(
                &self,
                options: Option<crate::ConversationMessageReaderOptions>,
            ) -> Result<Arc<MessageReader>, XmtpError> {
                let context = self.inner.context.clone();
                let group_id = self.inner.group_id;
                let client_key = self.client_key;
                on_sdk_worker(self.inner.context.clone(), async move {
                    MessageReader::open(
                        context,
                        xmtp_mls::subscriptions::local_delivery::DeliveryScope::Groups(vec![
                            group_id,
                        ]),
                        Default::default(),
                        options.unwrap_or_default().from,
                        client_key,
                    )
                })
                .await
            }
        }
    };
}

common_conversation!(Group, GroupState, GroupState::from_snapshot);
common_conversation!(Dm, ConversationState, |snapshot| Ok(
    ConversationState::from_snapshot(&snapshot)
));

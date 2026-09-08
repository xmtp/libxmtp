use crate::{ApiClientWrapper, ApiError, PublishUnit, Result, dyn_err};
use std::collections::HashMap;
use xmtp_api_d14n::envelope::*;
use xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT;
use xmtp_proto::{
    api_client::{XmtpBackendClient, XmtpMlsStreams},
    backend_v1 as wire,
    types::{
        Cursor, GroupId, GroupMessage, GroupMessageMetadata, InstallationId, Topic, TopicCursor,
        WelcomeMessage,
    },
};

#[derive(Clone, Debug)]
pub struct GroupFilter {
    pub group_id: GroupId,
    pub id_cursor: Option<u64>,
}
impl GroupFilter {
    pub fn new(group_id: GroupId, id_cursor: Option<u64>) -> Self {
        Self {
            group_id,
            id_cursor,
        }
    }
}

pub type KeyPackageMap = HashMap<InstallationId, Option<wire::KeyPackage>>;
type MessageMetadataMap = HashMap<GroupId, GroupMessageMetadata>;

impl<C: XmtpBackendClient> ApiClientWrapper<C> {
    #[xmtp_common::rpc_span]
    pub async fn query_group_messages(&self, group_id: GroupId) -> Result<Vec<GroupMessage>> {
        self.query_group_messages_with_cursors(HashMap::from([(
            Topic::new_group_message(group_id),
            Cursor(0),
        )]))
        .await
    }
    pub async fn query_group_messages_with_cursors(
        &self,
        cursors: TopicCursor,
    ) -> Result<Vec<GroupMessage>> {
        self.query_all(cursors, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
            .await?
            .into_iter()
            .map(|envelope| decode_group_message(envelope).map_err(Into::into))
            .collect()
    }
    #[xmtp_common::rpc_span]
    pub async fn query_latest_group_message(
        &self,
        group_id: GroupId,
    ) -> Result<Option<GroupMessage>> {
        let result = self
            .newest(vec![Topic::new_group_message(group_id)], true)
            .await?
            .into_iter()
            .next();
        result
            .map(|result| {
                decode_group_message(wire::ServerEnvelope {
                    meta: result.meta,
                    envelope: result.envelope,
                })
                .map_err(Into::into)
            })
            .transpose()
    }
    #[xmtp_common::rpc_span]
    pub async fn query_welcome_messages<Id: AsRef<[u8]> + Copy>(
        &self,
        installation_id: Id,
    ) -> Result<Vec<WelcomeMessage>> {
        self.query_welcome_messages_with_cursors(HashMap::from([(
            Topic::new_welcome_message(installation_id.as_ref().try_into()?),
            Cursor(0),
        )]))
        .await
    }
    pub async fn query_welcome_messages_with_cursors(
        &self,
        cursors: TopicCursor,
    ) -> Result<Vec<WelcomeMessage>> {
        self.query_all(cursors, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
            .await?
            .into_iter()
            .map(|envelope| decode_welcome_message(envelope).map_err(Into::into))
            .collect()
    }
    #[xmtp_common::rpc_span]
    pub async fn upload_key_package(&self, key_package: Vec<u8>) -> Result<wire::EnvelopeMeta> {
        self.publish_units(vec![PublishUnit::single(wire::ClientEnvelope {
            payload: Some(wire::client_envelope::Payload::KeyPackage(
                wire::KeyPackage {
                    key_package_tls_serialized: key_package,
                },
            )),
        })?])
        .await?
        .into_iter()
        .next()
        .ok_or(ApiError::InvalidResponse("key package metadata"))
    }
    #[xmtp_common::rpc_span]
    pub async fn fetch_key_packages(&self, keys: &[InstallationId]) -> Result<KeyPackageMap> {
        let mut found: KeyPackageMap = keys.iter().cloned().map(|key| (key, None)).collect();
        let topics = found.keys().map(Topic::new_key_package).collect();
        for result in self.newest(topics, true).await? {
            let topic = Topic::parse(
                &result
                    .topic
                    .ok_or(ApiError::InvalidResponse("key package topic"))?
                    .topic,
            )?;
            let key: InstallationId = topic.identifier().try_into()?;
            let slot = found
                .get_mut(&key)
                .ok_or(ApiError::InvalidResponse("unrequested key package"))?;
            if slot.is_some() {
                return Err(ApiError::InvalidResponse("duplicate key package"));
            }
            *slot = Some(decode_key_package(wire::ServerEnvelope {
                meta: result.meta,
                envelope: result.envelope,
            })?);
        }
        Ok(found)
    }
    #[xmtp_common::rpc_span]
    pub async fn send_welcome_messages(
        &self,
        messages: &[wire::WelcomeMessage],
    ) -> Result<Vec<wire::EnvelopeMeta>> {
        let units = messages
            .iter()
            .cloned()
            .map(|message| {
                PublishUnit::single(wire::ClientEnvelope {
                    payload: Some(wire::client_envelope::Payload::WelcomeMessage(message)),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.publish_units(units).await
    }
    #[xmtp_common::rpc_span]
    pub async fn send_group_messages(
        &self,
        units: Vec<PublishUnit>,
    ) -> Result<Vec<wire::EnvelopeMeta>> {
        self.publish_units(units).await
    }
    #[xmtp_common::rpc_span]
    pub async fn publish_commit_log(
        &self,
        entries: Vec<wire::CommitLogEntry>,
    ) -> Result<Vec<wire::EnvelopeMeta>> {
        let units = entries
            .into_iter()
            .map(|entry| {
                PublishUnit::single(wire::ClientEnvelope {
                    payload: Some(wire::client_envelope::Payload::CommitLogEntry(entry)),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.publish_units(units).await
    }
    #[xmtp_common::rpc_span]
    pub async fn query_commit_log(
        &self,
        cursors: TopicCursor,
    ) -> Result<Vec<xmtp_proto::types::CommitLogEntry>> {
        self.query_all(cursors, BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32)
            .await?
            .into_iter()
            .map(|envelope| decode_commit_log_entry(envelope).map_err(Into::into))
            .collect()
    }
    #[xmtp_common::rpc_span]
    pub async fn get_newest_message_metadata(
        &self,
        group_ids: &[GroupId],
    ) -> Result<MessageMetadataMap> {
        self.newest(
            group_ids.iter().map(Topic::new_group_message).collect(),
            false,
        )
        .await?
        .into_iter()
        .map(|result| {
            let meta = decode_group_message_metadata(
                result
                    .meta
                    .ok_or(ApiError::InvalidResponse("group metadata"))?,
            )?;
            Ok((meta.group_id, meta))
        })
        .collect()
    }
    #[xmtp_common::rpc_span]
    pub async fn get_envelope(&self, sequence_id: u64) -> Result<wire::ServerEnvelope> {
        self.retry_call(
            || self.api_client.get(wire::GetRequest { sequence_id }),
            false,
        )
        .await
        .map_err(dyn_err)
    }
}

impl<C: XmtpMlsStreams> ApiClientWrapper<C> {
    #[xmtp_common::rpc_span]
    pub async fn subscribe_group_messages(
        &self,
        groups: &[&GroupId],
    ) -> Result<C::GroupMessageStream> {
        self.retry_call(|| self.api_client.subscribe_group_messages(groups), false)
            .await
            .map_err(dyn_err)
    }
    #[xmtp_common::rpc_span]
    pub async fn subscribe_group_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<C::GroupMessageStream> {
        self.retry_call(
            || {
                self.api_client
                    .subscribe_group_messages_with_cursors(cursors)
            },
            false,
        )
        .await
        .map_err(dyn_err)
    }
    #[xmtp_common::rpc_span]
    pub async fn subscribe_welcome_messages(
        &self,
        installations: &[&InstallationId],
    ) -> Result<C::WelcomeMessageStream> {
        self.retry_call(
            || self.api_client.subscribe_welcome_messages(installations),
            false,
        )
        .await
        .map_err(dyn_err)
    }
    #[xmtp_common::rpc_span]
    pub async fn subscribe_welcome_messages_with_cursors(
        &self,
        cursors: &TopicCursor,
    ) -> Result<C::WelcomeMessageStream> {
        self.retry_call(
            || {
                self.api_client
                    .subscribe_welcome_messages_with_cursors(cursors)
            },
            false,
        )
        .await
        .map_err(dyn_err)
    }
}

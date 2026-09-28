#[derive(uniffi::Object)]
pub struct Group {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
    identity: ConversationIdentity,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) state_counts: Arc<parking_lot::Mutex<(u64, u64, u64)>>,
    #[cfg(test)]
    pub(crate) history_query_count: Arc<parking_lot::Mutex<u64>>,
}

#[derive(uniffi::Object)]
pub struct Dm {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
    identity: ConversationIdentity,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) state_counts: Arc<parking_lot::Mutex<(u64, u64, u64)>>,
    #[cfg(test)]
    pub(crate) history_query_count: Arc<parking_lot::Mutex<u64>>,
}

struct ConversationIdentity {
    added_by_inbox_id: InboxId,
    creator_inbox_id: InboxId,
    is_creator: bool,
}

impl ConversationIdentity {
    fn from_metadata(
        added_by_inbox_id: String,
        metadata: &xmtp_mls::mls_common::group_metadata::GroupMetadata,
        own_inbox_id: &str,
    ) -> Result<Self, XmtpError> {
        Ok(Self {
            added_by_inbox_id: InboxId::try_from(added_by_inbox_id)?,
            creator_inbox_id: InboxId::try_from(metadata.creator_inbox_id.clone())?,
            is_creator: metadata.creator_inbox_id == own_inbox_id,
        })
    }

    async fn from_core(
        group: &MlsGroup<xmtp_mls::MlsContext>,
    ) -> Result<(Self, xmtp_mls::mls_common::group_metadata::GroupMetadata), XmtpError> {
        let added_by_inbox_id =
            InboxId::try_from(group.added_by_inbox_id().map_err(XmtpError::unknown)?)?;
        let metadata = group.metadata().await.map_err(XmtpError::unknown)?;
        Ok((
            Self::from_metadata(added_by_inbox_id.0, &metadata, group.context.inbox_id())?,
            metadata,
        ))
    }
}

impl Group {
    pub(crate) async fn from_core(
        inner: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        let (identity, _) = ConversationIdentity::from_core(&inner).await?;
        Ok(Self {
            inner,
            client_key,
            identity,
            #[cfg(all(test, not(target_arch = "wasm32")))]
            state_counts: Arc::new(parking_lot::Mutex::new((0, 0, 0))),
            #[cfg(test)]
            history_query_count: Arc::new(parking_lot::Mutex::new(0)),
        })
    }
}

impl Dm {
    fn from_metadata(
        inner: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
        identity: ConversationIdentity,
        metadata: xmtp_mls::mls_common::group_metadata::GroupMetadata,
    ) -> Result<Self, XmtpError> {
        metadata
            .dm_members
            .ok_or_else(|| XmtpError::invalid("DM has no peer metadata"))?;
        Ok(Self {
            inner,
            client_key,
            identity,
            #[cfg(all(test, not(target_arch = "wasm32")))]
            state_counts: Arc::new(parking_lot::Mutex::new((0, 0, 0))),
            #[cfg(test)]
            history_query_count: Arc::new(parking_lot::Mutex::new(0)),
        })
    }

    async fn from_core(
        inner: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        let (identity, metadata) = ConversationIdentity::from_core(&inner).await?;
        Self::from_metadata(inner, client_key, identity, metadata)
    }
}

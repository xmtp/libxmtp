#[derive(uniffi::Object)]
pub struct Group {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
    identity: ConversationIdentity,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) state_counts: Arc<parking_lot::Mutex<(u64, u64, u64)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) history_query_count: Arc<parking_lot::Mutex<u64>>,
}

#[derive(uniffi::Object)]
pub struct Dm {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
    identity: ConversationIdentity,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) state_counts: Arc<parking_lot::Mutex<(u64, u64, u64)>>,
    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) history_query_count: Arc<parking_lot::Mutex<u64>>,
}

// A handle keeps the identity state it observed when it was created.
struct ConversationIdentity {
    added_by_inbox_id: Option<InboxId>,
    creator_inbox_id: Option<InboxId>,
    is_creator: bool,
}

// An empty received identity string means the value is unknown.
fn known_inbox_id(value: String) -> Option<InboxId> {
    (!value.is_empty()).then(|| InboxId::unchecked(value))
}

impl ConversationIdentity {
    fn from_metadata(
        added_by_inbox_id: String,
        metadata: &xmtp_mls::mls_common::group_metadata::GroupMetadata,
        own_inbox_id: &str,
        membership_state: xmtp_db::group::GroupMembershipState,
    ) -> Self {
        let creator_inbox_id = if membership_state == xmtp_db::group::GroupMembershipState::Restored
        {
            None
        } else {
            known_inbox_id(metadata.creator_inbox_id.clone())
        };
        Self {
            added_by_inbox_id: known_inbox_id(added_by_inbox_id),
            // An unknown creator is never the local inbox.
            is_creator: creator_inbox_id
                .as_ref()
                .is_some_and(|creator| creator.checked().is_ok_and(|text| text == own_inbox_id)),
            creator_inbox_id,
        }
    }

    async fn from_core(
        group: &MlsGroup<xmtp_mls::MlsContext>,
    ) -> Result<(Self, xmtp_mls::mls_common::group_metadata::GroupMetadata), XmtpError> {
        let stored = group
            .context
            .db()
            .find_group(&group.group_id)
            .map_err(xmtp_mls::groups::GroupError::from)
            .and_then(|stored| {
                stored.ok_or_else(|| xmtp_db::NotFound::GroupById(group.group_id).into())
            })
            .map_err(XmtpError::from_core)?;
        let metadata = group.metadata().await.map_err(XmtpError::from_core)?;
        Ok((
            Self::from_metadata(
                stored.added_by_inbox_id,
                &metadata,
                group.context.inbox_id(),
                stored.membership_state,
            ),
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
            #[cfg(all(test, not(target_arch = "wasm32")))]
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
            #[cfg(all(test, not(target_arch = "wasm32")))]
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

#[cfg(test)]
mod identity_tests {
    use super::*;
    use xmtp_mls::mls_common::group_metadata::GroupMetadata;

    const OWN: &str = "0000000000000000000000000000000000000000000000000000000000000001";
    const OTHER: &str = "0000000000000000000000000000000000000000000000000000000000000002";

    fn identity(creator: &str, adder: &str) -> ConversationIdentity {
        let metadata = GroupMetadata::new(ConversationType::Group, creator.into(), None, None);
        ConversationIdentity::from_metadata(
            adder.into(),
            &metadata,
            OWN,
            xmtp_db::group::GroupMembershipState::Allowed,
        )
    }

    // Each distinct received state keeps unknown values absent and known text exact.
    #[xmtp_common::test(unwrap_try = true)]
    fn received_identity_states_keep_unknown_values_absent() {
        let unknown_creator = identity("", OTHER);
        assert_eq!(unknown_creator.creator_inbox_id, None);
        assert_eq!(
            unknown_creator
                .added_by_inbox_id
                .map(|id| id.into_checked().unwrap()),
            Some(OTHER.into())
        );
        assert!(!unknown_creator.is_creator);

        let unknown_adder = identity(OWN, "");
        assert_eq!(
            unknown_adder
                .creator_inbox_id
                .map(|id| id.into_checked().unwrap()),
            Some(OWN.into())
        );
        assert_eq!(unknown_adder.added_by_inbox_id, None);
        assert!(unknown_adder.is_creator);

        let both_unknown = identity("", "");
        assert_eq!(both_unknown.creator_inbox_id, None);
        assert_eq!(both_unknown.added_by_inbox_id, None);
        assert!(!both_unknown.is_creator);

        let other_creator = identity(OTHER, OWN);
        assert!(!other_creator.is_creator);
    }
}

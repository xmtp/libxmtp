use crate::{XmtpError, client::CoreClient};
use xmtp_mls::subscriptions::internal::InternalEvent;
use xmtp_mls::{client::ClientError, mls_store::MlsStoreError};

use super::{EventContentTypeId, EventKind};

#[derive(Clone, Debug, Default, uniffi::Record)]
pub struct EventFilter {
    pub kinds: Vec<EventKind>,
    #[uniffi(default = None)]
    pub group_ids: Option<Vec<Vec<u8>>>,
    #[uniffi(default = None)]
    pub content_types: Option<Vec<EventContentTypeId>>,
    #[uniffi(default = false)]
    pub references_own_messages: bool,
}

impl EventFilter {
    /// Checks every group ID. Call this before any client or database
    /// access, so a malformed ID returns `InvalidArgument`.
    pub(crate) fn checked(self) -> Result<CheckedEventFilter, XmtpError> {
        let group_ids = self
            .group_ids
            .map(|ids| {
                ids.into_iter()
                    .map(|id| {
                        xmtp_proto::types::GroupId::try_from(id)
                            .map_err(|_| XmtpError::invalid_argument("invalid event group ID"))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        Ok(CheckedEventFilter {
            kinds: self.kinds,
            group_ids,
            content_types: self.content_types,
            references_own_messages: self.references_own_messages,
        })
    }
}

/// An event filter whose group IDs are valid.
pub(crate) struct CheckedEventFilter {
    kinds: Vec<EventKind>,
    group_ids: Option<Vec<xmtp_proto::types::GroupId>>,
    content_types: Option<Vec<EventContentTypeId>>,
    references_own_messages: bool,
}

impl CheckedEventFilter {
    pub(crate) fn to_core(
        &self,
        client: &CoreClient,
    ) -> Result<xmtp_events::EventFilter<InternalEvent>, XmtpError> {
        let mut filter = xmtp_events::EventFilter::new(self.kinds.iter().copied().map(Into::into));
        if let Some(ids) = &self.group_ids {
            let mut group_ids = Vec::with_capacity(ids.len());
            for group_id in ids {
                match client.group(group_id) {
                    Ok(group) => {
                        if let Some(dm_id) = group.dm_id {
                            filter.dm_identifiers.push(dm_id.into_bytes());
                        }
                    }
                    Err(ClientError::MlsStore(MlsStoreError::NotFound(_))) => {}
                    Err(error) => return Err(XmtpError::from_client(error)),
                }
                group_ids.push(group_id.as_slice().to_vec());
            }
            filter.group_ids = Some(group_ids);
        }
        filter.content_types = self.content_types.as_ref().map(|ids| {
            ids.iter()
                .map(|id| xmtp_events::ContentTypeId {
                    authority_id: id.authority_id.clone(),
                    type_id: id.type_id.clone(),
                    version_major: id.version_major,
                })
                .collect()
        });
        filter.references_own_messages = self.references_own_messages;
        Ok(filter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(ids: Vec<Vec<u8>>) -> EventFilter {
        EventFilter {
            group_ids: Some(ids),
            ..Default::default()
        }
    }

    /// An event filter takes 32-byte (keyed) and 16-byte (legacy) group
    /// IDs, and refuses every other length before any client access.
    #[xmtp_common::test(unwrap_try = true)]
    fn event_filter_accepts_both_group_id_lengths() {
        let checked = filter(vec![vec![1; 32], vec![2; 16]]).checked()?;
        let ids = checked.group_ids.expect("group IDs were given");
        assert_eq!(ids[0].as_slice(), &[1; 32]);
        assert_eq!(ids[1].as_slice(), &[2; 16]);
        for len in [0, 8, 15, 17, 31, 33] {
            let error = filter(vec![vec![1; 32], vec![1; len]])
                .checked()
                .err()
                .expect("a wrong-length group ID is refused");
            assert!(matches!(error, XmtpError::InvalidArgument(_)), "{error:?}");
        }
    }
}

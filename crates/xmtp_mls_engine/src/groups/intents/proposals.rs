//! Proposal intents: member updates, group context extensions, and committing
//! pending proposals.

use super::*;

/// Intent data for proposing member updates (adds and/or removes) to a group
#[derive(Debug, Clone)]
pub(crate) struct ProposeMemberUpdateIntentData {
    pub add_inbox_ids: Vec<String>,
    pub remove_inbox_ids: Vec<String>,
}

impl ProposeMemberUpdateIntentData {
    pub fn new(add_inbox_ids: Vec<String>, remove_inbox_ids: Vec<String>) -> Self {
        Self {
            add_inbox_ids,
            remove_inbox_ids,
        }
    }
}

impl TryFrom<ProposeMemberUpdateIntentData> for Vec<u8> {
    type Error = IntentError;
    fn try_from(intent: ProposeMemberUpdateIntentData) -> Result<Self, Self::Error> {
        let decode_inbox_ids =
            |inbox_ids: Vec<String>, item: &'static str| -> Result<Vec<Vec<u8>>, IntentError> {
                inbox_ids
                    .into_iter()
                    .map(|s| {
                        hex::decode(&s)
                            .map_err(|_| xmtp_proto::ConversionError::InvalidValue {
                                item,
                                expected: "hex encoded string",
                                got: s,
                            })
                            .map_err(Into::into)
                    })
                    .collect::<Result<Vec<_>, _>>()
            };
        let proposal = ProposeMemberUpdateData {
            version: Some(propose_member_update_data::Version::V1(
                propose_member_update_data::V1 {
                    add_inbox_ids: decode_inbox_ids(intent.add_inbox_ids, "add_inbox_ids")?,
                    remove_inbox_ids: decode_inbox_ids(
                        intent.remove_inbox_ids,
                        "remove_inbox_ids",
                    )?,
                },
            )),
        }
        .encode_to_vec();
        Ok(proposal)
    }
}

impl TryFrom<&[u8]> for ProposeMemberUpdateIntentData {
    type Error = IntentError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let proto = ProposeMemberUpdateData::decode(data)?;
        let decode_inbox_ids = |inbox_ids: Vec<Vec<u8>>| -> Vec<String> {
            inbox_ids
                .into_iter()
                .map(|b| hex::encode(&b))
                .collect::<Vec<_>>()
        };
        match proto.version {
            Some(propose_member_update_data::Version::V1(v1)) => {
                let add_inbox_ids = decode_inbox_ids(v1.add_inbox_ids);
                let remove_inbox_ids = decode_inbox_ids(v1.remove_inbox_ids);
                Ok(Self::new(add_inbox_ids, remove_inbox_ids))
            }
            None => Err(IntentError::MissingPayload),
        }
    }
}

/// Intent data for proposing group context extension updates (proposal-by-reference flow)
#[derive(Debug, Clone)]
pub(crate) struct ProposeGroupContextExtensionsIntentData {
    /// The serialized extensions bytes
    pub extensions_bytes: Vec<u8>,
}

impl ProposeGroupContextExtensionsIntentData {
    pub fn new(extensions_bytes: Vec<u8>) -> Self {
        Self { extensions_bytes }
    }
}

impl From<ProposeGroupContextExtensionsIntentData> for Vec<u8> {
    fn from(intent: ProposeGroupContextExtensionsIntentData) -> Self {
        ProposeGroupContextExtensionData {
            version: Some(propose_group_context_extension_data::Version::V1(
                propose_group_context_extension_data::V1 {
                    group_context_extension: intent.extensions_bytes,
                },
            )),
        }
        .encode_to_vec()
    }
}

impl TryFrom<&[u8]> for ProposeGroupContextExtensionsIntentData {
    type Error = IntentError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let proto = ProposeGroupContextExtensionData::decode(data)?;
        match proto.version {
            Some(propose_group_context_extension_data::Version::V1(v1)) => {
                Ok(Self::new(v1.group_context_extension))
            }
            None => Err(IntentError::MissingPayload),
        }
    }
}

/// Intent data for committing pending proposals (proposal-by-reference flow)
#[derive(Debug, Clone, Default)]
pub(crate) struct CommitPendingProposalsIntentData {
    // Empty for now - commits all pending proposals in the proposal store
}

impl CommitPendingProposalsIntentData {
    pub fn new() -> Self {
        Self {}
    }
}

impl From<CommitPendingProposalsIntentData> for Vec<u8> {
    fn from(_intent: CommitPendingProposalsIntentData) -> Self {
        CommitPendingProposalsData {
            version: Some(commit_pending_proposals_data::Version::V1(
                commit_pending_proposals_data::V1 {
                    proposal_hashes: vec![],
                },
            )),
        }
        .encode_to_vec()
    }
}

impl TryFrom<&[u8]> for CommitPendingProposalsIntentData {
    type Error = IntentError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        // Handle empty data for backwards compatibility and default case
        if data.is_empty() {
            return Ok(Self::new());
        }
        let proto = CommitPendingProposalsData::decode(data)?;
        match proto.version {
            Some(commit_pending_proposals_data::Version::V1(_)) => Ok(Self::new()),
            None => Err(IntentError::MissingPayload),
        }
    }
}

//! Test-only snapshots of one group's state for convergence checks.
//!
//! IDs and public MLS state use hex so reports can cross a process boundary.
//! No secret MLS state, group key, or message contents leave these
//! snapshots.

use crate::{
    context::XmtpSharedContext,
    groups::{GroupError, MlsGroup},
};
use openmls::prelude::BasicCredential;
use serde::{Deserialize, Serialize};
use tls_codec::Serialize as _;
use xmtp_cryptography::hash::sha256_array;
use xmtp_db::{
    ConnectionExt, NotFound, TransactionOutcome, XmtpMlsStorageProvider,
    group::GroupMembershipState, prelude::*,
};
use xmtp_mls_common::app_data::{
    components::inbox_id_set::{AdminListComponent, SuperAdminListComponent},
    typed_facade::MlsGroupAppData,
};
use xmtp_proto::types::GroupId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemberSnapshot {
    pub inbox_id: String,
    pub installation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommitSnapshot {
    pub sequence_id: i64,
    pub epoch: i64,
    pub authenticator: String,
    pub last_authenticator: String,
    pub result: i32,
    pub commit_type: Option<String>,
    pub removed_this_installation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupSnapshot {
    pub group_id: String,
    /// Zero with an empty authenticator when a Restored archive stub has no MLS state.
    pub epoch: u64,
    pub epoch_authenticator: String,
    pub members: Vec<MemberSnapshot>,
    /// Admin inbox IDs in the committed dictionary.
    #[serde(default)]
    pub admins: Vec<String>,
    /// Super admin inbox IDs in the committed dictionary.
    #[serde(default)]
    pub super_admins: Vec<String>,
    /// SHA-256 of the serialized group context extensions, so peers can
    /// compare metadata and app data. A digest, not the bytes: the AppData
    /// dictionary holds the group root.
    pub metadata: String,
    pub membership_state: GroupMembershipState,
    pub active: bool,
    pub maybe_forked: bool,
    pub is_commit_log_forked: Option<bool>,
    pub cursor: u64,
    pub commits: Vec<CommitSnapshot>,
    /// Older local records excluded by the per-snapshot commit limit.
    #[serde(default)]
    pub omitted_commit_count: u64,
}

/// Maximum commit records sent in one group snapshot.
pub(crate) const SNAPSHOT_COMMIT_LIMIT: i64 = 256;

pub(crate) fn recent_commit_records(
    db: &impl DbQuery,
    group_id: &GroupId,
) -> Result<(Vec<xmtp_db::local_commit_log::LocalCommitLog>, u64), xmtp_db::ConnectionError> {
    use xmtp_db::diesel::prelude::*;
    use xmtp_db::schema::local_commit_log::dsl;

    ConnectionExt::raw_query(db, |connection| {
        let total = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .count()
            .get_result::<i64>(connection)?;
        let mut records = dsl::local_commit_log
            .filter(dsl::group_id.eq(group_id))
            .order(dsl::rowid.desc())
            .limit(SNAPSHOT_COMMIT_LIMIT)
            .load::<xmtp_db::local_commit_log::LocalCommitLog>(connection)?;
        // Row order preserves a re-add's sequence-zero Welcome among later commits.
        records.reverse();
        let omitted = total.saturating_sub(records.len() as i64) as u64;
        Ok((records, omitted))
    })
}

impl<C: XmtpSharedContext> MlsGroup<C> {
    /// Read every field under one database writer, including cross-process writers.
    pub fn diagnostic_snapshot(&self) -> Result<GroupSnapshot, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let stored = db
                .find_group(&self.group_id)?
                .ok_or(NotFound::GroupById(self.group_id))?;
            let (records, omitted_commit_count) = recent_commit_records(&db, &self.group_id)?;
            let commits = records
                .into_iter()
                .map(|entry| CommitSnapshot {
                    sequence_id: entry.commit_sequence_id,
                    epoch: entry.applied_epoch_number,
                    authenticator: hex::encode(entry.applied_epoch_authenticator),
                    last_authenticator: hex::encode(entry.last_epoch_authenticator),
                    result: entry.commit_result as i32,
                    removed_this_installation: entry.commit_type.as_deref()
                        == Some("RemovedFromGroup"),
                    commit_type: entry.commit_type,
                })
                .collect();
            let group = openmls::group::MlsGroup::load(&storage, &self.group_id.to_openmls())?;
            // An archive stub can owe membership without having MLS state yet.
            let Some(group) = group else {
                if stored.membership_state != GroupMembershipState::Restored {
                    return Err(GroupError::Storage(
                        NotFound::MlsGroup(self.group_id).into(),
                    ));
                }
                return Ok(TransactionOutcome::Continue(GroupSnapshot {
                    group_id: hex::encode(self.group_id),
                    epoch: 0,
                    epoch_authenticator: String::new(),
                    members: Vec::new(),
                    admins: Vec::new(),
                    super_admins: Vec::new(),
                    metadata: String::new(),
                    membership_state: stored.membership_state,
                    active: false,
                    maybe_forked: stored.maybe_forked,
                    is_commit_log_forked: stored.is_commit_log_forked,
                    cursor: db
                        .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
                            self.group_id,
                        ))?
                        .processed
                        .0,
                    commits,
                    omitted_commit_count,
                }));
            };
            let mut members = group
                .members()
                .map(|member| {
                    let credential = BasicCredential::try_from(member.credential)?;
                    Ok(MemberSnapshot {
                        inbox_id: crate::identity::parse_credential(credential.identity())?,
                        installation_id: hex::encode(member.signature_key),
                    })
                })
                .collect::<Result<Vec<_>, GroupError>>()?;
            members.sort();
            let roles = MlsGroupAppData::new(group.extensions());
            let admins: Vec<String> = roles
                .get::<AdminListComponent>()?
                .map(|set| set.iter().map(|id| id.to_hex()).collect())
                .unwrap_or_default();
            let super_admins: Vec<String> = roles
                .get::<SuperAdminListComponent>()?
                .map(|set| set.iter().map(|id| id.to_hex()).collect())
                .unwrap_or_default();
            Ok::<_, GroupError>(TransactionOutcome::Continue(GroupSnapshot {
                group_id: hex::encode(self.group_id),
                epoch: group.epoch().as_u64(),
                epoch_authenticator: hex::encode(group.epoch_authenticator().as_slice()),
                members,
                admins,
                super_admins,
                // implements: GMOD-042
                metadata: hex::encode(sha256_array(&group.extensions().tls_serialize_detached()?)),
                membership_state: stored.membership_state,
                active: stored.membership_state != GroupMembershipState::Restored
                    && group.is_active(),
                maybe_forked: stored.maybe_forked,
                is_commit_log_forked: stored.is_commit_log_forked,
                cursor: db
                    .topic_progress(&xmtp_db::incoming_envelope::StreamTopic::group(
                        self.group_id,
                    ))?
                    .processed
                    .0,
                commits,
                omitted_commit_count,
            }))
        })
        .map(TransactionOutcome::into_continued)
    }
}

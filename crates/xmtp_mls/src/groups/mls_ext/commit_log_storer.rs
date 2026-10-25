use crate::StorageError;
use crate::groups::GroupError;
use crate::groups::{mls_sync::GroupMessageProcessingError, validated_commit::ValidatedCommit};
use crate::identity::Identity;
use openmls::group::{MlsGroup, MlsGroupCreateConfig, StagedCommit};
use openmls::prelude::CredentialWithKey;
use openmls::prelude::GroupEpoch;
use openmls::prelude::StagedWelcome;
use prost::Message;
use std::collections::HashMap;
use xmtp_content_types::{ContentCodec, group_updated::GroupUpdatedCodec};
use xmtp_db::MlsProviderExt;
use xmtp_db::group_message::{ContentType, GroupMessageKind, MsgQueryArgs};
use xmtp_db::refresh_state::EntityKind;
use xmtp_db::{
    Store, XmtpMlsStorageProvider,
    local_commit_log::{CommitType, NewLocalCommitLog},
    prelude::*,
    remote_commit_log::CommitResult,
};
use xmtp_proto::types::Cursor;
use xmtp_proto::xmtp::mls::message_contents::EncodedContent;

use xmtp_mls_validation::{ForkActors, commit::inbox_id_from_credential};
use xmtp_proto::types::GroupId;
/// This trait wraps openmls groups to include commit logs for any mutations to encryption state.
/// This helps with fork detection.
pub trait CommitLogStorer: std::marker::Sized {
    /// Create a new group named `group_id`, the group signing key of its
    /// root.
    fn from_creation_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
        commit_log_enabled: bool,
    ) -> Result<Self, GroupError>;

    fn from_backup_stub_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
        commit_log_enabled: bool,
    ) -> Result<Self, GroupError>;

    fn from_welcome_logged(
        provider: &impl MlsProviderExt,
        welcome: StagedWelcome,
        sender_inbox_id: &str,
        sender_installation_id: &[u8],
        commit_log_enabled: bool,
    ) -> Result<Self, GroupError>;

    fn merge_staged_commit_logged(
        &mut self,
        provider: &impl MlsProviderExt,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
        commit_log_enabled: bool,
    ) -> Result<(), GroupMessageProcessingError>;

    /// Recover legacy publication eligibility once, in the caller's state transaction.
    fn recover_commit_log_publisher(
        &self,
        db: &impl xmtp_db::DbQuery,
        own_inbox: &str,
    ) -> Result<bool, GroupMessageProcessingError>;

    /// Marks a commit as failed in the commit log.
    /// Only call this when the status of the commit is final.
    /// Specifically, do not call this for retryable errors, or
    /// VersionTooLow/GroupPaused errors.
    fn mark_failed_commit_logged(
        &self,
        provider: &impl MlsProviderExt,
        commit_cursor: u64,
        commit_epoch: GroupEpoch,
        error: &GroupMessageProcessingError,
        commit_log_enabled: bool,
    ) -> Result<(), StorageError>;
}

impl CommitLogStorer for MlsGroup {
    // implements: GMOD-040
    fn from_creation_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
        commit_log_enabled: bool,
    ) -> Result<Self, GroupError> {
        let mls_group = MlsGroup::new_with_group_id(
            provider,
            &identity.installation_keys,
            group_config,
            group_id.to_openmls(),
            CredentialWithKey {
                credential: identity.credential(),
                signature_key: identity.installation_keys.public_slice().into(),
            },
        )?;

        if commit_log_enabled {
            NewLocalCommitLog {
                group_id: mls_group.group_id().try_into()?,
                commit_sequence_id: 0,
                last_epoch_authenticator: vec![],
                commit_result: CommitResult::Success,
                applied_epoch_number: mls_group.epoch().as_u64() as i64,
                applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: Some(identity.inbox_id().to_string()),
                sender_installation_id: Some(identity.installation_id().to_vec()),
                commit_type: Some(format!("{}", CommitType::GroupCreation)),
                error_message: None,
            }
            .store(&provider.key_store().db())?;
        }

        // A newly created membership has no legacy history to recover.
        provider.key_store().db().update_cursor(
            group_id,
            EntityKind::CommitLogPublisherRecovery,
            Cursor(1),
        )?;

        Ok(mls_group)
    }

    fn from_backup_stub_logged(
        provider: &impl MlsProviderExt,
        identity: &Identity,
        group_config: &MlsGroupCreateConfig,
        group_id: GroupId,
        commit_log_enabled: bool,
    ) -> Result<Self, GroupError> {
        let mls_group = MlsGroup::new_with_group_id(
            provider,
            &identity.installation_keys,
            group_config,
            group_id.to_openmls(),
            CredentialWithKey {
                credential: identity.credential(),
                signature_key: identity.installation_keys.public_slice().into(),
            },
        )?;

        if commit_log_enabled {
            // It is safe to log this stubbed encryption state, because we will not upload anything
            // to the remote commit log with a sequence ID of 0.
            NewLocalCommitLog {
                group_id: mls_group.group_id().try_into()?,
                commit_sequence_id: 0,
                last_epoch_authenticator: vec![],
                commit_result: CommitResult::Success,
                applied_epoch_number: mls_group.epoch().as_u64() as i64,
                applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: None,
                sender_installation_id: None,
                commit_type: Some(format!("{}", CommitType::BackupRestore)),
                error_message: None,
            }
            .store(&provider.key_store().db())?;
        }

        Ok(mls_group)
    }

    fn from_welcome_logged(
        provider: &impl MlsProviderExt,
        welcome: StagedWelcome,
        sender_inbox_id: &str,
        sender_installation_id: &[u8],
        commit_log_enabled: bool,
    ) -> Result<Self, GroupError> {
        // Failed welcomes do not need to be added to the commit log
        let mls_group = welcome.into_group(provider)?;

        if commit_log_enabled {
            NewLocalCommitLog {
                group_id: mls_group.group_id().try_into()?,
                // TODO(rich): Replace with the cursor sequence ID of the welcome once implemented
                commit_sequence_id: 0,
                last_epoch_authenticator: vec![],
                commit_result: CommitResult::Success,
                applied_epoch_number: mls_group.epoch().as_u64() as i64,
                applied_epoch_authenticator: mls_group.epoch_authenticator().as_slice().to_vec(),
                sender_inbox_id: Some(sender_inbox_id.to_string()),
                sender_installation_id: Some(sender_installation_id.to_vec()),
                commit_type: Some(format!("{}", CommitType::Welcome)),
                error_message: None,
            }
            .store(&provider.key_store().db())?;
        }

        Ok(mls_group)
    }

    // implements: FORK-001
    fn merge_staged_commit_logged(
        &mut self,
        provider: &impl MlsProviderExt,
        staged_commit: StagedCommit,
        validated_commit: &ValidatedCommit,
        sequence_id: i64,
        commit_log_enabled: bool,
    ) -> Result<(), GroupMessageProcessingError> {
        // Whether this commit removes our own leaf (authored by anyone — an
        // admin removal or our own leave request). Captured before the merge
        // consumes the staged commit.
        let removed_us = staged_commit.self_removed();
        let last_epoch_number = self.epoch().as_u64() as i64;
        let last_epoch_authenticator = self.epoch_authenticator().as_slice().to_vec();
        let metadata = &validated_commit.metadata_changes;
        let publisher_inbox = inbox_id_from_credential(
            self.credential()
                .map_err(|_| GroupMessageProcessingError::GroupInactive)?,
        )?;
        let db = provider.key_store().db();
        let permitted_before = self.recover_commit_log_publisher(&db, &publisher_inbox)?;
        let publishers_before = if !metadata.super_admins_added.is_empty()
            || !metadata.super_admins_removed.is_empty()
            || validated_commit.installations_changed
            || !validated_commit.readded_installations.is_empty()
        {
            Some(ForkActors::from_group(self)?.publisher_keys(self)?)
        } else {
            None
        };
        self.merge_staged_commit(provider, staged_commit)?;
        // implements: FORK-085
        // Only an actual MLS role transition skips earlier local history.
        // Repairing a stale cached flag must preserve eligible pending entries.
        let group_id = self.group_id().try_into()?;
        let actors = ForkActors::from_group(self)?;
        let permits = self.is_active() && actors.permits(&publisher_inbox);
        if permits && !permitted_before {
            let cursor = db.get_local_commit_log_cursor(&group_id)?.unwrap_or(0);
            db.update_cursor(group_id, EntityKind::CommitLogUpload, Cursor(cursor as u64))?;
        }
        db.set_should_publish_commit_log(&group_id, permits)?;
        // implements: FORK-084
        if let Some(previous) = publishers_before {
            let publishers = actors.publisher_keys(self)?;
            let mut boundaries = db.get_commit_log_publisher_boundaries(&group_id)?;
            boundaries.retain(|author, _| publishers.contains(author));
            for author in publishers {
                if !previous.contains(&author)
                    || validated_commit.readded_installations.contains(&author)
                {
                    boundaries.insert(author, sequence_id);
                } else {
                    boundaries.entry(author).or_insert(0);
                }
            }
            db.set_commit_log_publisher_boundaries(&group_id, &boundaries)?;
        }
        // The merge emptied the proposal store; its delivery evidence goes with it.
        provider
            .key_store()
            .db()
            .forget_received_proposals(self.group_id().try_into()?)?;
        let applied_epoch_authenticator = self.epoch_authenticator().as_slice().to_vec();

        if removed_us {
            // A commit that removes us merges only the public diff (openmls
            // `StagedCommitState::PublicState`): the group context epoch
            // advances and the group becomes inactive, but a removed member
            // cannot derive the new epoch's secrets. Record what is actually
            // true — we processed the commit, remained at the pre-commit
            // epoch, and exited — instead of claiming we applied the new
            // epoch with a stale authenticator (which fork detection would
            // misread as a fork; see commit-log fork investigation).
            if commit_log_enabled {
                NewLocalCommitLog {
                    group_id: self.group_id().try_into()?,
                    commit_sequence_id: sequence_id,
                    last_epoch_authenticator: last_epoch_authenticator.clone(),
                    commit_result: CommitResult::Success,
                    applied_epoch_number: last_epoch_number,
                    applied_epoch_authenticator: last_epoch_authenticator,
                    sender_inbox_id: Some(validated_commit.actor_inbox_id()),
                    sender_installation_id: Some(validated_commit.actor_installation_id()),
                    commit_type: Some(format!("{}", CommitType::RemovedFromGroup)),
                    error_message: None,
                }
                .store(&provider.key_store().db())?;
            }
            return Ok(());
        }

        // Invariant: a successful staged-commit merge by a member that
        // remains in the group always advances the epoch and therefore
        // changes the epoch authenticator. If it did not, the group state we
        // merged onto was corrupt/torn (e.g. a cross-process race on a shared
        // MLS DB handed us a reloaded group that already carried the
        // post-commit epoch secrets). Recording a Success entry would persist
        // a forked commit log. Refuse instead: the error is retryable, so the
        // enclosing transaction (cursor advance + merge + log write) rolls
        // back and the message is reprocessed against settled state.
        if applied_epoch_authenticator == last_epoch_authenticator {
            // No tracing here: the error carries the full context and is
            // logged when it surfaces at its destination.
            return Err(GroupMessageProcessingError::EpochAuthenticatorNotAdvanced {
                group_id: self.group_id().try_into()?,
                commit_sequence_id: sequence_id,
                epoch: self.epoch().as_u64(),
            });
        }

        if commit_log_enabled {
            NewLocalCommitLog {
                group_id: self.group_id().try_into()?,
                commit_sequence_id: sequence_id,
                last_epoch_authenticator,
                commit_result: CommitResult::Success,
                applied_epoch_number: self.epoch().as_u64() as i64,
                applied_epoch_authenticator,
                sender_inbox_id: Some(validated_commit.actor_inbox_id()),
                sender_installation_id: Some(validated_commit.actor_installation_id()),
                commit_type: Some(format!("{}", validated_commit.debug_commit_type())),
                error_message: None,
            }
            .store(&provider.key_store().db())?;
        }

        Ok(())
    }

    // implements: FORK-085, FORK-086
    fn recover_commit_log_publisher(
        &self,
        db: &impl xmtp_db::DbQuery,
        own_inbox: &str,
    ) -> Result<bool, GroupMessageProcessingError> {
        let group_id = self.group_id().try_into()?;
        let actors = ForkActors::from_group(self)?;
        let permits = self.is_active() && actors.permits(own_inbox);
        if db
            .get_refresh_state(group_id, EntityKind::CommitLogPublisherRecovery)?
            .is_some()
        {
            return Ok(permits);
        }
        let logs = db.get_group_logs(&group_id)?;
        let mut cutoff = logs.last().map_or(0, |entry| i64::from(entry.rowid));
        if permits {
            let messages = if matches!(actors, ForkActors::SuperAdmins(_)) {
                db.get_group_messages(
                    &group_id,
                    &MsgQueryArgs {
                        kind: Some(GroupMessageKind::MembershipChange),
                        content_types: Some(vec![ContentType::GroupUpdated]),
                        ..Default::default()
                    },
                )?
                .into_iter()
                // Archive conversion marks imported messages should_push=false;
                // locally applied membership transcripts keep this bit true.
                .filter(|message| message.group_id == group_id && message.should_push)
                .map(|message| (message.sequence_id, message))
                .collect::<HashMap<_, _>>()
            } else {
                HashMap::new()
            };
            let mut epoch = self.epoch().as_u64();
            let mut authenticator = self.epoch_authenticator().as_slice().to_vec();
            for entry in logs.iter().rev() {
                // A historical marker alone cannot identify this membership:
                // logging may have been disabled for a replacement Welcome.
                if entry.commit_type.as_deref() == Some("RemovedFromGroup")
                    || entry.commit_sequence_id == 0
                    || entry.applied_epoch_number as u64 != epoch
                    || entry.applied_epoch_authenticator != authenticator
                {
                    break;
                }
                if entry.commit_result == CommitResult::Success {
                    if epoch == 0 {
                        break;
                    }
                    epoch -= 1;
                    authenticator.clone_from(&entry.last_epoch_authenticator);
                } else if entry.last_epoch_authenticator != authenticator {
                    break;
                }
                cutoff = i64::from(entry.rowid) - 1;
                if matches!(actors, ForkActors::Dm(_))
                    || entry.commit_result != CommitResult::Success
                {
                    continue;
                }
                // Only locally generated membership records can prove a role
                // change. App messages with GroupUpdated content cannot do so.
                let update = messages.get(&entry.commit_sequence_id).and_then(|message| {
                    // Undecodable retained evidence makes this role change
                    // unresolved; it must not authorize the earlier prefix.
                    let Ok(content) =
                        EncodedContent::decode(message.decrypted_message_bytes.as_slice())
                    else {
                        return None;
                    };
                    let Ok(update) = GroupUpdatedCodec::decode(content) else {
                        return None;
                    };
                    Some(update)
                });
                match update {
                    Some(update)
                        if update
                            .added_super_admin_inboxes
                            .iter()
                            .any(|inbox| inbox.inbox_id == own_inbox) =>
                    {
                        break;
                    }
                    Some(_) => {}
                    None if matches!(
                        entry.commit_type.as_deref(),
                        Some("KeyUpdate" | "MetadataUpdate")
                    ) => {}
                    // The last unresolved applied change may itself have made
                    // us a publisher. Its post-state is proven; its prefix is not.
                    None => break,
                }
            }
        }
        db.update_cursor(group_id, EntityKind::CommitLogUpload, Cursor(cutoff as u64))?;
        db.set_should_publish_commit_log(&group_id, permits)?;
        db.update_cursor(group_id, EntityKind::CommitLogPublisherRecovery, Cursor(1))?;
        Ok(permits)
    }

    fn mark_failed_commit_logged(
        &self,
        provider: &impl MlsProviderExt,
        commit_sequence_id: u64,
        commit_epoch: GroupEpoch,
        error: &GroupMessageProcessingError,
        commit_log_enabled: bool,
    ) -> Result<(), StorageError> {
        if !commit_log_enabled {
            return Ok(());
        }
        let group_id: GroupId = self.group_id().try_into()?;
        let last_epoch_number = self.epoch();
        let last_epoch_authenticator = self.epoch_authenticator();
        let conn = provider.key_store().db();
        let mut maybe_recently_welcomed = true;
        // Latest log may not exist if a client upgraded from a version without local commit logs
        if let Some(latest_log) = conn.get_latest_log_for_group(&group_id)?
            && latest_log.commit_type != Some(CommitType::Welcome.to_string())
        {
            maybe_recently_welcomed = false;
        }
        // If we've recently joined the group, we may get a bunch of wrong epoch errors
        // until we 'catch up' to the commit that spawned the welcome. We can ignore these for now.
        if commit_epoch.as_u64() <= last_epoch_number.as_u64() && maybe_recently_welcomed {
            return Ok(());
        }

        NewLocalCommitLog {
            group_id,
            commit_sequence_id: commit_sequence_id as i64,
            last_epoch_authenticator: last_epoch_authenticator.as_slice().to_vec(),
            commit_result: error.commit_result(),
            applied_epoch_number: last_epoch_number.as_u64() as i64,
            applied_epoch_authenticator: last_epoch_authenticator.as_slice().to_vec(),
            error_message: Some(format!("{:?}", error)),
            sender_inbox_id: None,
            sender_installation_id: None,
            commit_type: None,
        }
        .store(&conn)?;
        Ok(())
    }
}

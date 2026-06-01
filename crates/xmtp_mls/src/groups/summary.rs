use chrono::Utc;
use derive_builder::Builder;
use openmls::group::GroupContext;
use std::collections::{HashMap, HashSet};
use xmtp_common::RetryableError;
use xmtp_db::group_intent::IntentKind;
use xmtp_proto::types::Cursor;

use super::{GroupError, mls_sync::GroupMessageProcessingError};
use xmtp_proto::types::GroupId;

#[derive(Default)]
pub struct SyncSummary {
    pub(crate) publish_errors: Vec<GroupError>,
    pub(crate) process: ProcessSummary,
    pub(crate) post_commit_errors: Vec<GroupError>,
    /// an error outside of the sync occurred
    pub(crate) other: Option<Box<GroupError>>,
}

impl RetryableError for SyncSummary {
    fn is_retryable(&self) -> bool {
        self.publish_errors.iter().any(|e| e.is_retryable())
            || self.post_commit_errors.iter().any(|e| e.is_retryable())
            || self
                .other
                .as_ref()
                .map(|s| s.is_retryable())
                .unwrap_or(false)
    }
}

impl SyncSummary {
    /// synced a single message successfully
    pub fn single(msg: MessageIdentifier) -> Self {
        let mut process = ProcessSummary::default();
        process.add(msg);
        SyncSummary {
            process,
            ..Default::default()
        }
    }

    /// Try to get a newly processed message by its cursor ID
    pub fn new_message_by_id(&self, id: Cursor) -> Option<&MessageIdentifier> {
        self.process.new_messages.iter().find(|m| m.cursor == id)
    }

    /// Whether the sync *operation* failed (publish, post-commit, or an
    /// unrelated error). This is the single predicate that decides whether a
    /// summary may sit in the `Err` position and which Display branch is used —
    /// keep the two in lockstep.
    ///
    /// Note: per-message processing failures (`process.errored`) are
    /// deliberately *not* counted here. A sync that decrypts some messages and
    /// fails others is still a successful sync of the group as a whole; those
    /// failures are reported inside the summary, not by flipping it to an error.
    pub fn is_errored(&self) -> bool {
        self.other.is_some()
            || !self.publish_errors.is_empty()
            || !self.post_commit_errors.is_empty()
    }

    pub fn add_publish_err(&mut self, e: GroupError) {
        self.publish_errors.push(e);
    }

    pub fn add_post_commit_err(&mut self, e: GroupError) {
        self.post_commit_errors.push(e);
    }

    pub fn add_process(&mut self, process: ProcessSummary) {
        self.process = process;
    }

    pub fn extend(&mut self, other: SyncSummary) {
        self.publish_errors.extend(other.publish_errors);
        self.process.extend(other.process);
        self.post_commit_errors.extend(other.post_commit_errors);
        // Preserve the first non-None cause. `extend` is called once per retry
        // round in `sync_until_intent_resolved_inner`; overwriting here would let
        // a later clean round clobber an earlier round's `other` error, losing
        // the cause on the timeout (SyncFailedToWait) path.
        if self.other.is_none() {
            self.other = other.other;
        }
    }

    /// Construct a sync which failed with an unrelated error.
    pub fn other(err: GroupError) -> Self {
        Self {
            other: Some(Box::new(err)),
            ..Default::default()
        }
    }

    pub fn add_other(&mut self, err: GroupError) {
        self.other = Some(Box::new(err));
    }
}

impl std::error::Error for SyncSummary {
    /// Surface the first underlying cause so the error chain (e.g. the FFI
    /// `Caused by:` walk) reaches the real failure instead of dead-ending at
    /// the summary's flattened Display string. Prefer the sync-operation
    /// errors, falling back to the first per-message processing error.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Some(other) = self.other.as_deref() {
            return Some(other);
        }
        if let Some(err) = self.publish_errors.first() {
            return Some(err);
        }
        if let Some(err) = self.post_commit_errors.first() {
            return Some(err);
        }
        self.process
            .errored
            .first()
            .map(|(_, e)| e as &dyn std::error::Error)
    }
}

impl std::fmt::Debug for SyncSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self)
    }
}

impl std::fmt::Display for SyncSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.is_errored() {
            let first_new = self
                .process
                .new_messages
                .iter()
                .min_by_key(|k| k.cursor)
                .map(|m| m.cursor);
            write!(
                f,
                "synced {} messages, {} failed {} succeeded from cursor {:?}",
                self.process.total_messages.len(),
                self.process.errored.len(),
                self.process.new_messages.len(),
                first_new
            )?;
            if !self.process.errored.is_empty() {
                write!(f, "{}", self.process.unique_errors())?;
            }
        } else {
            writeln!(
                f,
                "================================= Errors Occurred During Sync ==========================="
            )?;
            if !self.publish_errors.is_empty() {
                writeln!(f, "{} errors publishing intents", self.publish_errors.len())?;
            }
            if !self.post_commit_errors.is_empty() {
                writeln!(f, "{} errors post commit", self.post_commit_errors.len())?;
            }
            if let Some(e) = &self.other {
                writeln!(f, "{}", e)?;
            }
            writeln!(f, "{}", self.process)?;
            writeln!(
                f,
                "========================================================================================"
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Builder)]
#[builder(setter(into), build_fn(error = "GroupMessageProcessingError"))]
pub struct MessageIdentifier {
    /// the cursor of the message as it exists on the network
    pub cursor: Cursor,
    pub group_id: GroupId,
    pub created_ns: chrono::DateTime<Utc>,
    /// true if the message has been processed previously
    #[builder(default = false)]
    pub previously_processed: bool,
    /// the id of the message in the local database
    #[builder(default = None)]
    pub internal_id: Option<Vec<u8>>,
    /// The context of the MLS Group from this message
    /// Indicates that the message is a commit
    #[builder(default = None)]
    pub group_context: Option<GroupContext>,
    /// The kind of intent processed, if this was our own message
    #[builder(default = None)]
    pub intent_kind: Option<IntentKind>,
}

impl MessageIdentifier {
    pub fn builder() -> MessageIdentifierBuilder {
        Default::default()
    }
}

impl std::fmt::Debug for MessageIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageIdentifier")
            .field("cursor", &self.cursor)
            .field("group_id", &xmtp_common::fmt::debug_hex(self.group_id))
            .field("created_ns", &self.created_ns)
            .field("internal_id", &self.internal_id)
            .field("context", &self.group_context.as_ref().map(|g| g.epoch()))
            .field("intent", &self.intent_kind)
            .finish()
    }
}

impl From<&xmtp_proto::types::GroupMessage> for MessageIdentifierBuilder {
    fn from(value: &xmtp_proto::types::GroupMessage) -> Self {
        MessageIdentifierBuilder {
            cursor: Some(value.cursor),
            group_id: Some(value.group_id),
            created_ns: Some(value.created_ns),
            internal_id: None,
            group_context: None,
            intent_kind: None,
            previously_processed: Some(false),
        }
    }
}

impl From<&xmtp_proto::types::GroupMessage> for MessageIdentifier {
    fn from(value: &xmtp_proto::types::GroupMessage) -> Self {
        MessageIdentifier {
            cursor: value.cursor,
            group_id: value.group_id,
            created_ns: value.created_ns,
            internal_id: None,
            group_context: None,
            intent_kind: None,
            previously_processed: false,
        }
    }
}

impl PartialOrd for MessageIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.cursor.partial_cmp(&other.cursor)
    }
}

/// Information about which messages could be synced,
/// And which messages could not be synced.
#[derive(Default)]
pub struct ProcessSummary {
    pub total_messages: HashSet<Cursor>,
    pub new_messages: Vec<MessageIdentifier>,
    pub errored: Vec<(Cursor, GroupMessageProcessingError)>,
}

impl std::fmt::Debug for ProcessSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self)
    }
}

pub struct ErrorSet {
    // Hashmap of message ids and the error they failed with
    unique: HashMap<String, Vec<Cursor>>,
    /// sorted vector of all failed ids
    sorted_ids: Vec<(Cursor, String)>,
}

impl std::fmt::Display for ErrorSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (error_msg, ids) in &self.unique {
            let mut sorted_ids = ids.clone();
            sorted_ids.sort();
            write!(f, "\n\t┝━> {:?} {error_msg}", sorted_ids)?;
        }
        Ok(())
    }
}

impl ErrorSet {
    pub fn unique(&self) -> &HashMap<String, Vec<Cursor>> {
        &self.unique
    }

    pub fn sorted(&self) -> &[(Cursor, String)] {
        &self.sorted_ids
    }
}

impl ProcessSummary {
    pub fn add_id(&mut self, cursor: Cursor) {
        self.total_messages.insert(cursor);
    }

    pub fn add(&mut self, message: MessageIdentifier) {
        self.total_messages.insert(message.cursor);
        self.new_messages.push(message);
    }

    /// the last message processed
    pub fn last(&self) -> Option<Cursor> {
        self.total_messages.iter().max().copied()
    }

    /// the first messages processed
    pub fn first(&self) -> Option<Cursor> {
        self.total_messages.iter().min().copied()
    }

    /// Total messages processed
    pub fn total(&self) -> usize {
        self.total_messages.len()
    }

    /// number of new messages processed
    pub fn new_message(&self) -> usize {
        self.new_messages.len()
    }

    /// the cursor of the first decryptable message
    pub fn first_new(&self) -> Option<Cursor> {
        self.new_messages.iter().map(|m| m.cursor).min()
    }

    /// the latest message that failed
    pub fn last_errored(&self) -> Option<Cursor> {
        self.errored.iter().map(|(i, _)| *i).max()
    }

    pub fn errored(&mut self, cursor: Cursor, error: GroupMessageProcessingError) {
        self.errored.push((cursor, error));
    }

    pub fn unique_errors(&self) -> ErrorSet {
        let mut sorted = self
            .errored
            .iter()
            .map(|(m, e)| (*m, e.to_string()))
            .collect::<Vec<(_, String)>>();
        sorted.sort_by_key(|(m, _)| *m);
        let mut error_set: HashMap<String, Vec<Cursor>> = HashMap::new();
        for (id, err) in sorted.iter().cloned() {
            error_set.entry(err).or_default().push(id);
        }
        ErrorSet {
            unique: error_set,
            sorted_ids: sorted,
        }
    }

    pub fn extend(&mut self, other: ProcessSummary) {
        self.total_messages.extend(other.total_messages);
        self.new_messages.extend(other.new_messages);
        self.errored.extend(other.errored)
    }

    pub fn is_errored(&self) -> bool {
        !self.errored.is_empty()
    }

    /// detailed printout of the messages processed
    fn detailed(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error_set = self.unique_errors();
        writeln!(
            f,
            "\n=========================== Processed Messages Summary  ====================="
        )?;
        writeln!(
            f,
            "Processed {} total messages in cursor range [{:?} ... {:?}]",
            self.total_messages.len(),
            error_set.sorted_ids.first().map(|(m, _)| m),
            error_set.sorted_ids.last().map(|(m, _)| m)
        )?;
        if !self.errored.is_empty() {
            let error_ids = error_set.unique.values().flatten();
            let min = error_ids.clone().min();
            let max = error_ids.clone().max();

            writeln!(
                f,
                "Failed to process [{}] messages in cursor range [{:?} ... {:?}]\n\
                [{}] unique errors:",
                self.errored.len(),
                min,
                max,
                error_set.unique.len(),
            )?;
            for (err, ids) in error_set.unique.iter() {
                writeln!(f, "{} ids errored with [{}]", ids.len(), err)?;
            }
        } else {
            writeln!(f, "no errors encountered processing messages.")?;
        }
        let success_range = self.new_messages.iter().map(|m| m.cursor);
        let min = success_range.clone().min();
        let max = success_range.clone().max();
        writeln!(
            f,
            "Successfully processed {} messages in range {:?} ... {:?}",
            self.new_messages.len(),
            min,
            max
        )?;
        write!(
            f,
            "=============================================================================",
        )?;

        Ok(())
    }
}

impl std::fmt::Display for ProcessSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.total_messages.len() > 1 {
            self.detailed(f)?;
        } else {
            write!(
                f,
                "processed {} total messages, ",
                self.total_messages.len()
            )?;
            if !self.errored.is_empty() {
                for (cursor, message) in self.errored.iter() {
                    write!(f, "message with cursor {cursor} failed with {}", message)?;
                }
            }
            if !self.new_messages.is_empty() {
                write!(
                    f,
                    "{} new decryptable message(s) received",
                    self.new_messages.len()
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod extend_tests {
    use super::*;

    // `extend` is called once per retry round in
    // `sync_until_intent_resolved_inner`. A later clean round must not clobber an
    // earlier round's `other` cause — otherwise the timeout (SyncFailedToWait)
    // path loses the real failure.
    #[xmtp_common::test]
    fn extend_preserves_first_other_cause() {
        let mut acc = SyncSummary::default();
        acc.extend(SyncSummary::other(GroupError::GroupInactive)); // round 1: cause
        acc.extend(SyncSummary::default()); // round 2: clean — must not clobber

        let other = acc.other.as_ref().expect("first cause must survive");
        assert_eq!(other.to_string(), GroupError::GroupInactive.to_string());
    }

    #[xmtp_common::test]
    fn extend_takes_other_when_none_yet() {
        let mut acc = SyncSummary::default();
        acc.extend(SyncSummary::default()); // round 1: clean
        acc.extend(SyncSummary::other(GroupError::GroupInactive)); // round 2: cause appears

        assert!(acc.other.is_some(), "a later cause is still captured");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::mls_sync::GroupMessageProcessingError;
    use std::error::Error;

    // The Display "success" banner must fire iff is_errored() is false, and a
    // summary may only be in the Err position when is_errored() is true. These
    // two had drifted (Display used all-three-empty; is_errored() used && for
    // publish/post_commit), so a publish-only failure printed the error banner
    // while still reporting as not-errored. Lock the predicates together.
    fn errored_banner(s: &SyncSummary) -> bool {
        s.to_string().contains("Errors Occurred During Sync")
    }

    #[xmtp_common::test]
    fn clean_summary_is_not_errored() {
        let summary = SyncSummary::default();
        assert!(!summary.is_errored());
        assert!(!errored_banner(&summary));
        assert!(summary.source().is_none());
    }

    #[xmtp_common::test]
    fn publish_only_error_is_errored() {
        let mut summary = SyncSummary::default();
        summary.add_publish_err(GroupError::GroupInactive);
        // Regression: with `&&` this returned false while Display showed the banner.
        assert!(summary.is_errored());
        assert!(errored_banner(&summary));
    }

    #[xmtp_common::test]
    fn post_commit_only_error_is_errored() {
        let mut summary = SyncSummary::default();
        summary.add_post_commit_err(GroupError::GroupInactive);
        assert!(summary.is_errored());
        assert!(errored_banner(&summary));
    }

    #[xmtp_common::test]
    fn other_error_is_errored_and_is_source() {
        let mut summary = SyncSummary::default();
        summary.add_other(GroupError::GroupInactive);
        assert!(summary.is_errored());
        // `other` is the highest-priority cause surfaced through the chain.
        let source = summary.source().expect("source should be present");
        assert_eq!(source.to_string(), GroupError::GroupInactive.to_string());
    }

    #[xmtp_common::test]
    fn per_message_failures_do_not_flip_errored() {
        // A sync that fails to process some messages is still a successful sync
        // of the group as a whole — it stays Ok and prints the success line.
        let mut process = ProcessSummary::default();
        process.errored(
            Cursor::new(7u64, 0u32),
            GroupMessageProcessingError::InvalidPayload,
        );
        let mut summary = SyncSummary::default();
        summary.add_process(process);

        assert!(!summary.is_errored());
        assert!(!errored_banner(&summary));
        // ...but the failure is still reachable as the cause of last resort.
        let source = summary
            .source()
            .expect("per-message error is the fallback source");
        assert_eq!(
            source.to_string(),
            GroupMessageProcessingError::InvalidPayload.to_string()
        );
    }

    #[xmtp_common::test]
    fn source_prefers_other_over_per_message_error() {
        let mut process = ProcessSummary::default();
        process.errored(
            Cursor::new(7u64, 0u32),
            GroupMessageProcessingError::InvalidPayload,
        );
        let mut summary = SyncSummary::default();
        summary.add_process(process);
        summary.add_publish_err(GroupError::GroupInactive);

        let source = summary.source().expect("source should be present");
        assert_eq!(source.to_string(), GroupError::GroupInactive.to_string());
    }
}

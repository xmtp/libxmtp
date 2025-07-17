use derive_builder::Builder;
use openmls::group::GroupContext;
use std::collections::{HashMap, HashSet};
use xmtp_common::RetryableError;
use xmtp_db::group_intent::IntentKind;
use xmtp_proto::mls_v1::group_message;

use super::{mls_sync::GroupMessageProcessingError, GroupError};

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
    pub fn new_message_by_id(&self, id: u64) -> Option<&MessageIdentifier> {
        self.process.new_messages.iter().find(|m| m.cursor == id)
    }

    pub fn is_errored(&self) -> bool {
        self.other.is_some()
            || (!self.publish_errors.is_empty() && !self.post_commit_errors.is_empty())
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
        self.other = other.other;
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
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }

    fn cause(&self) -> Option<&dyn std::error::Error> {
        self.source()
    }
}

impl std::fmt::Debug for SyncSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self)
    }
}

impl std::fmt::Display for SyncSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.publish_errors.is_empty()
            && self.post_commit_errors.is_empty()
            && self.other.is_none()
        {
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
    pub cursor: u64,
    pub group_id: Vec<u8>,
    pub created_ns: u64,
    /// tru if the message has been processed previously
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
            .field("group_id", &xmtp_common::fmt::debug_hex(&self.group_id))
            .field("created_ns", &self.created_ns)
            .field("internal_id", &self.internal_id)
            .field("context", &self.group_context.as_ref().map(|g| g.epoch()))
            .field("intent", &self.intent_kind)
            .finish()
    }
}

impl From<&group_message::V1> for MessageIdentifierBuilder {
    fn from(value: &group_message::V1) -> Self {
        MessageIdentifierBuilder {
            cursor: Some(value.id),
            group_id: Some(value.group_id.clone()),
            created_ns: Some(value.created_ns),
            internal_id: None,
            group_context: None,
            intent_kind: None,
            previously_processed: Some(false),
        }
    }
}

impl From<&group_message::V1> for MessageIdentifier {
    fn from(value: &group_message::V1) -> Self {
        MessageIdentifier {
            cursor: value.id,
            group_id: value.group_id.clone(),
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
    pub total_messages: HashSet<u64>,
    pub new_messages: Vec<MessageIdentifier>,
    pub errored: Vec<(u64, GroupMessageProcessingError)>,
}

impl std::fmt::Debug for ProcessSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self)
    }
}

pub struct ErrorSet {
    // Hashmap of message ids and the error they failed with
    unique: HashMap<String, Vec<u64>>,
    /// sorted vector of all failed ids
    sorted_ids: Vec<(u64, String)>,
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
    pub fn unique(&self) -> &HashMap<String, Vec<u64>> {
        &self.unique
    }

    pub fn sorted(&self) -> &[(u64, String)] {
        &self.sorted_ids
    }
}

impl ProcessSummary {
    pub fn add_id(&mut self, id: u64) {
        self.total_messages.insert(id);
    }

    pub fn add(&mut self, message: MessageIdentifier) {
        self.total_messages.insert(message.cursor);
        self.new_messages.push(message);
    }

    /// the last message processed
    pub fn last(&self) -> Option<u64> {
        self.total_messages.iter().max().copied()
    }

    /// the first messages processed
    pub fn first(&self) -> Option<u64> {
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
    pub fn first_new(&self) -> Option<u64> {
        self.new_messages.iter().map(|m| m.cursor).min()
    }

    /// the latest message that failed
    pub fn last_errored(&self) -> Option<u64> {
        self.errored.iter().map(|(i, _)| *i).max()
    }

    pub fn errored(&mut self, message_id: u64, error: GroupMessageProcessingError) {
        self.errored.push((message_id, error));
    }

    pub fn unique_errors(&self) -> ErrorSet {
        let mut sorted = self
            .errored
            .iter()
            .map(|(m, e)| (*m, e.to_string()))
            .collect::<Vec<(_, String)>>();
        sorted.sort_by_key(|(m, _)| *m);
        let mut error_set: HashMap<String, Vec<u64>> = HashMap::new();
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

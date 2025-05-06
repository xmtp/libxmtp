use std::collections::HashMap;

use xmtp_common::RetryableError;
use xmtp_db::group_intent::IntentState;

use super::{mls_sync::GroupMessageProcessingError, GroupError};

#[derive(Default)]
pub struct SyncSummary {
    publish_errors: Vec<GroupError>,
    pub process: ProcessSummary,
    post_commit_errors: Vec<GroupError>,
    /// an error outside of the sync occurred
    other: Option<Box<GroupError>>,
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
        if self.publish_errors.is_empty() && self.post_commit_errors.is_empty() {
            write!(
                f,
                "synced {} messages, {} failed {} succeeded",
                self.process.total_messages.len(),
                self.process.errored.len(),
                self.process.new_messages.len()
            )?;
        } else {
            writeln!(
                f,
                "================================= Errors Occured During Sync ==========================="
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

/// The originating source of a message.
/// It either originated from us (in which case has an associated IntentState)
/// or it is an external message.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MessageSource {
    /// Only own published messages
    Own(IntentState),
    /// External Messages
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageIdentifier {
    /// the cursor of the message as it exists on the network
    pub cursor: u64,
    /// The generated ID of the message as it exists in the database
    id: Vec<u8>,
    /// The source of the message
    source: MessageSource,
}

impl PartialOrd for MessageIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.cursor.partial_cmp(&other.cursor)
    }
}

impl MessageIdentifier {
    pub fn new(cursor: u64, id: Vec<u8>, source: MessageSource) -> Self {
        Self { cursor, id, source }
    }
}

/// Information about which messages could be synced,
/// And which messages could not be synced.
#[derive(Default)]
pub struct ProcessSummary {
    pub total_messages: Vec<u64>,
    /// vector of ids
    pub new_messages: Vec<MessageIdentifier>,
    pub errored: Vec<(u64, GroupMessageProcessingError)>,
}
impl std::fmt::Debug for ProcessSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self)
    }
}

impl ProcessSummary {
    pub fn add_id(&mut self, id: u64) {
        self.total_messages.push(id);
    }

    pub fn add(&mut self, message: MessageIdentifier) {
        self.new_messages.push(message);
    }

    pub fn errored(&mut self, message_id: u64, error: GroupMessageProcessingError) {
        self.errored.push((message_id, error));
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
        let mut sorted = self
            .errored
            .iter()
            .map(|(m, e)| (m, e.to_string()))
            .collect::<Vec<(_, String)>>();
        sorted.sort_by_key(|(m, _)| **m);
        let mut error_set: HashMap<String, Vec<u64>> = HashMap::new();
        for (id, err) in sorted.iter().cloned() {
            error_set
                .entry(err)
                .or_insert_with(Vec::new)
                .push(id.clone());
        }
        writeln!(
            f,
            "\n=========================== Processed Messages Summary  ====================="
        )?;
        writeln!(
            f,
            "Processed {} total messages in cursor range [{:?} ... {:?}]",
            self.total_messages.len(),
            sorted.first().map(|(m, _)| m),
            sorted.last().map(|(m, _)| m)
        )?;
        if !self.errored.is_empty() {
            let error_ids = error_set.values().flatten();
            let min = error_ids.clone().min();
            let max = error_ids.clone().max();

            writeln!(
                f,
                "Failed to process [{}] messages in cursor range [{:?} ... {:?}]\n\
                [{}] unique errors:",
                self.errored.len(),
                min,
                max,
                error_set.len(),
            )?;
            for (err, ids) in error_set.iter() {
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
            "Succesfully processed {} messages in range {:?} ... {:?}",
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
                    write!(
                        f,
                        "message with cursor {cursor} failed with {}",
                        message.to_string()
                    )?;
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

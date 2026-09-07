use std::{collections::{HashMap, HashSet, VecDeque}, sync::{Arc, Mutex}};
use tokio::sync::Notify;
use tonic::Status;
use crate::{db::StoredEnvelope, config::DELIVERY_FRAME_BYTES};
use super::{ENVELOPE_OVERHEAD, output::{Budget, Reservation, Terminal}};

pub(super) struct LiveBatch {
    pub rows: Vec<(u64, Arc<StoredEnvelope>)>,
    pub reservation: Reservation,
}
#[derive(Default)]
pub(super) struct Mail {
    pub heads: HashMap<Vec<u8>, (u64, i64)>,
    pub live: VecDeque<LiveBatch>,
}
pub(super) struct Mailbox {
    pub mail: Mutex<Mail>,
    pub wake: Notify,
    pub budget: Budget,
    pub terminal: Arc<Terminal>,
    pub frame_bytes: usize,
}
impl Default for Mailbox {
    fn default() -> Self { Self { mail: Mutex::new(Mail::default()), wake: Notify::new(), budget: Budget::default(), terminal: Arc::new(Terminal::default()), frame_bytes: DELIVERY_FRAME_BYTES } }
}
struct Watcher { generation: u64, current: bool }
struct Client { mailbox: Arc<Mailbox>, topics: HashSet<Vec<u8>> }
#[derive(Default)]
struct State {
    topics: HashMap<Vec<u8>, HashMap<u64, Watcher>>,
    clients: HashMap<u64, Client>,
    next_id: u64,
    ready: bool,
}
#[derive(Default)]
pub(crate) struct Registry(Mutex<State>);

impl Registry {
    pub fn ready(&self) { self.0.lock().expect("registry mutex").ready = true; }
    /// Enroll an empty session only after recovery is ready.
    pub(super) fn connect(&self, mailbox: Arc<Mailbox>) -> Result<u64, Status> {
        let mut state = self.0.lock().expect("registry mutex");
        if !state.ready { return Err(Status::unavailable("stream recovery in progress")); }
        state.next_id = state.next_id.checked_add(1).ok_or_else(|| Status::resource_exhausted("session identifiers exhausted"))?;
        let id = state.next_id;
        state.clients.insert(id, Client { mailbox, topics: HashSet::new() });
        Ok(id)
    }
    /// Register before target capture. Catching-up registrations retain heads,
    /// not payloads, until the owner completes the history-to-live handoff.
    pub(super) fn add(&self, id: u64, topic: Vec<u8>, generation: u64) -> Result<(), Status> {
        let mut state = self.0.lock().expect("registry mutex");
        if !state.ready { return Err(Status::unavailable("stream recovery in progress")); }
        state.clients.get_mut(&id).ok_or_else(|| Status::unavailable("session closed"))?.topics.insert(topic.clone());
        state.topics.entry(topic).or_default().insert(id, Watcher { generation, current: false });
        Ok(())
    }
    /// Remove shared interest and pending notices before acknowledging removal.
    /// Already queued payloads are rejected by the owner's generation check.
    pub(super) fn remove(&self, id: u64, topic: &[u8]) {
        let mut state = self.0.lock().expect("registry mutex");
        if let Some(watchers) = state.topics.get_mut(topic) { watchers.remove(&id); if watchers.is_empty() { state.topics.remove(topic); } }
        if let Some(client) = state.clients.get_mut(&id) { client.topics.remove(topic); client.mailbox.mail.lock().expect("mail mutex").heads.remove(topic); }
    }
    /// Atomically consume the latest notice and switch to direct payload delivery
    /// only if the owner's admitted floor covers it.
    pub(super) fn current(&self, id: u64, topic: &[u8], floor: i64) -> i64 {
        let mut state = self.0.lock().expect("registry mutex");
        let needed = state.clients.get(&id).and_then(|client| client.mailbox.mail.lock().expect("mail mutex").heads.remove(topic)).map_or(floor, |(_, head)| head.max(floor));
        if needed <= floor && let Some(watcher) = state.topics.get_mut(topic).and_then(|watchers| watchers.get_mut(&id)) { watcher.current = true; }
        needed
    }
    pub(super) fn disconnect(&self, id: u64) {
        let mut state = self.0.lock().expect("registry mutex");
        if let Some(client) = state.clients.remove(&id) { for topic in client.topics {
            if let Some(watchers) = state.topics.get_mut(&topic) { watchers.remove(&id); if watchers.is_empty() { state.topics.remove(&topic); } }
        } }
    }
    /// Publish a terminal error before clearing recovery registrations.
    pub fn fail_all(&self, error: Status) {
        let mut state = self.0.lock().expect("registry mutex");
        state.ready = false;
        for client in state.clients.values() { client.mailbox.terminal.fail(error.clone()); }
        state.topics.clear();
        state.clients.clear();
    }
    /// Share each committed row across interested sessions without awaiting them.
    /// A full mailbox fails that session instead of blocking the shared tailer.
    pub(super) fn dispatch(&self, rows: Vec<StoredEnvelope>) {
        let state = self.0.lock().expect("registry mutex");
        let mut live: HashMap<u64, Vec<(u64, Arc<StoredEnvelope>)>> = HashMap::new();
        for row in rows {
            let row = Arc::new(row);
            if let Some(watchers) = state.topics.get(&row.topic) { for (&id, watcher) in watchers {
                if watcher.current { live.entry(id).or_default().push((watcher.generation, row.clone())); }
                else if let Some(client) = state.clients.get(&id) {
                    let mut mail = client.mailbox.mail.lock().expect("mail mutex");
                    let head = mail.heads.entry(row.topic.clone()).or_insert((watcher.generation, row.sequence_id));
                    head.1 = head.1.max(row.sequence_id);
                    client.mailbox.wake.notify_one();
                }
            } }
        }
        for (id, rows) in live { if let Some(client) = state.clients.get(&id) { enqueue(&client.mailbox, rows); } }
    }
}

fn enqueue(mailbox: &Mailbox, rows: Vec<(u64, Arc<StoredEnvelope>)>) {
    let mut pending = Vec::new();
    let mut bytes = 0;
    for row in rows {
        let size = row.1.payload.len() + ENVELOPE_OVERHEAD;
        if bytes + size > mailbox.frame_bytes && !pending.is_empty() { flush(mailbox, std::mem::take(&mut pending), bytes); bytes = 0; }
        bytes += size;
        pending.push(row);
    }
    if !pending.is_empty() { flush(mailbox, pending, bytes); }
}
fn flush(mailbox: &Mailbox, rows: Vec<(u64, Arc<StoredEnvelope>)>, bytes: usize) {
    if let Some(reservation) = mailbox.budget.reserve(bytes) {
        mailbox.mail.lock().expect("mail mutex").live.push_back(LiveBatch { rows, reservation });
        mailbox.wake.notify_one();
    } else { mailbox.terminal.fail(Status::resource_exhausted("stream output capacity exceeded")); }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(topic: &[u8], id: i64) -> StoredEnvelope {
        StoredEnvelope { sequence_id: id, topic: topic.to_vec(), server_ns: 1, expiry_ns: None,
            message_hash: vec![0; 32], is_commit_or_proposal: false, payload: vec![1] }
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn catching_up_coalesces_heads_while_current_sessions_share_the_same_payload() {
        let registry = Registry::default();
        registry.ready();
        let first = Arc::new(Mailbox::default());
        let second = Arc::new(Mailbox::default());
        let catching_up = Arc::new(Mailbox::default());
        let a = registry.connect(first.clone())?;
        let b = registry.connect(second.clone())?;
        let c = registry.connect(catching_up.clone())?;
        let topic = vec![1; 33];
        for id in [a, b, c] { registry.add(id, topic.clone(), 1)?; }
        registry.dispatch(vec![row(&topic, 1), row(&topic, 2)]);
        assert!(catching_up.mail.lock().unwrap().live.is_empty());
        assert_eq!(registry.current(a, &topic, 0), 2);
        assert_eq!(registry.current(a, &topic, 2), 2);
        assert_eq!(registry.current(b, &topic, 2), 2);
        registry.dispatch(vec![row(&topic, 3)]);
        let left = first.mail.lock().unwrap().live.pop_front().unwrap();
        let right = second.mail.lock().unwrap().live.pop_front().unwrap();
        assert!(Arc::ptr_eq(&left.rows[0].1, &right.rows[0].1));
        assert_eq!(left.rows[0].1.sequence_id, 3);
        let pending = catching_up.mail.lock().unwrap();
        assert_eq!(pending.heads[&topic], (1, 3));
        assert!(pending.live.is_empty());
    }
}

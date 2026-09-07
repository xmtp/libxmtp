use super::{
    ENVELOPE_OVERHEAD, FETCH_TOPICS, OUTBOUND_FRAMES, StreamHub, fetch,
    keepalive::{Bucket, Challenge},
    output::{Frame, NativeOutput, Reservation},
    registry::{LiveBatch, Mailbox},
};
use crate::{
    api,
    config::{Config, DELIVERY_FRAME_BYTES},
    db,
    stream::fetch::Request,
};
use futures::{
    FutureExt, StreamExt,
    future::{BoxFuture, pending},
};
use prost::Message;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use tokio::sync::mpsc;
use tonic::{Status, Streaming};
use tracing::{Instrument, instrument::WithSubscriber};
use xmtp_common::time::{Duration, Instant, sleep};
use xmtp_proto::types::Topic;

struct Registration {
    generation: u64,
    floor: i64,
    needed: i64,
    queued: bool,
    fetching: bool,
    acknowledged: bool,
}
struct PendingUpdate {
    id: u64,
    removed_topics: usize,
    topics: Vec<Vec<u8>>,
    heads: BoxFuture<'static, Result<Vec<i64>, Status>>,
}
struct PendingFetch {
    requests: Vec<Request>,
    page: BoxFuture<'static, Result<fetch::ResultPage, Status>>,
}
struct Session {
    request_id: uuid::Uuid,
    hub: Arc<StreamHub>,
    id: u64,
    config: Arc<Config>,
    mailbox: Arc<Mailbox>,
    output: mpsc::Sender<Frame>,
    topics: HashMap<Vec<u8>, Registration>,
    ready: VecDeque<(Vec<u8>, u64)>,
    fetching: Option<PendingFetch>,
    pending_update: Option<PendingUpdate>,
    deferred_updates: VecDeque<api::subscribe_request::Update>,
    deferred_bytes: usize,
    update_id: u64,
    updates: Bucket,
    pings: Bucket,
    send_idle: Instant,
    challenge: Option<Challenge>,
    nonce: u64,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.hub.registry.disconnect(self.id);
    }
}

/// Open one native ingestion session. Dropping its output wakes the owner,
/// cancels pending fetches, and removes every shared registration.
pub(crate) fn native(
    hub: Arc<StreamHub>,
    config: Arc<Config>,
    input: Streaming<api::SubscribeRequest>,
    request_id: uuid::Uuid,
) -> Result<NativeOutput, Status> {
    let mailbox = Arc::new(Mailbox {
        frame_bytes: DELIVERY_FRAME_BYTES.min(config.limits.max_response_bytes.saturating_add(5)),
        ..Default::default()
    });
    let id = hub.registry.connect(mailbox.clone())?;
    let (output, receiver) = mpsc::channel(OUTBOUND_FRAMES);
    let session = Session {
        hub,
        id,
        request_id,
        config: config.clone(),
        mailbox: mailbox.clone(),
        output,
        topics: HashMap::new(),
        ready: VecDeque::new(),
        fetching: None,
        update_id: 0,
        pending_update: None,
        deferred_updates: VecDeque::new(),
        deferred_bytes: 0,
        updates: Bucket::new(
            config.limits.max_update_frames_per_second,
            config.limits.max_update_burst,
        ),
        pings: Bucket::new(
            config.limits.max_ping_frames_per_second,
            config.limits.max_ping_burst,
        ),
        send_idle: Instant::now(),
        challenge: None,
        nonce: 0,
    };
    let terminal = mailbox.terminal.clone();
    let span = tracing::Span::current();
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let task = tokio::spawn(
        async move {
            if let Err(error) = session.run(input).await {
                mailbox.terminal.fail(error);
            }
        }
        .instrument(span)
        .with_subscriber(dispatch),
    );
    Ok(NativeOutput::new(receiver, terminal, task))
}

impl Session {
    /// Serialize control, targets, history, live data, and timers in one owner.
    /// Target reads and history remain cancellable while input is processed.
    async fn run(mut self, mut input: Streaming<api::SubscribeRequest>) -> Result<(), Status> {
        self.control(api::subscribe_response::Response::Started(
            api::subscribe_response::Started {
                keepalive_interval_ms: self.config.streams.keepalive_interval_ms as u32,
            },
        ))?;
        let mut pending_input = VecDeque::new();
        loop {
            if self.mailbox.terminal.closed() {
                return Ok(());
            }
            if let Some(error) = self.mailbox.terminal.error() {
                return Err(error);
            }
            self.mail()?;
            if let Some(frame) = pending_input.pop_front() {
                self.input(frame)?;
                continue;
            }
            if self.pending_update.is_none()
                && let Some(update) = self.deferred_updates.pop_front()
            {
                self.deferred_bytes -= update.encoded_len();
                self.update(update)?;
            }
            if self.fetching.is_none() {
                self.fetching = self.start_fetch();
            }
            let timer = self.timer();
            let mailbox = self.mailbox.clone();
            tokio::select! {
                _ = mailbox.terminal.wake.notified() => {},
                frame = input.next() => match frame {
                    Some(frame) => self.input(frame?)?,
                    None => return Ok(()),
                },
                heads = async { match &mut self.pending_update { Some(update) => (&mut update.heads).await, None => pending().await } } => {
                    self.applied(heads?)?;
                },
                page = async { match &mut self.fetching { Some(fetch) => (&mut fetch.page).await, None => pending().await } } => {
                    self.fetching = None;
                    self.fetched(page?)?;
                },
                handed = async { match &mut self.challenge {
                    Some(challenge) if challenge.deadline.is_none() => (&mut challenge.handed).await.ok(),
                    _ => pending().await,
                }} => {
                    if let (Some(sent), Some(challenge)) = (handed, &mut self.challenge) {
                        challenge.start_deadline(sent, Duration::from_millis(self.config.streams.max_pong_wait_ms))?;
                    }
                },
                _ = mailbox.wake.notified() => {},
                _ = mailbox.budget.wake.notified() => {},
                _ = timer => self.on_timer(&mut input, &mut pending_input)?,
            }
        }
    }

    fn timer(&self) -> BoxFuture<'static, ()> {
        let deadline = match &self.challenge {
            Some(challenge) => challenge.deadline,
            None => Some(
                self.send_idle + Duration::from_millis(self.config.streams.keepalive_interval_ms),
            ),
        };
        match deadline {
            Some(deadline) => sleep(deadline.saturating_duration_since(Instant::now())).boxed(),
            None => pending().boxed(),
        }
    }

    /// Inspect already buffered input before failing an expired challenge.
    /// Other frames remain ordered for later processing; inbound traffic is not activity.
    fn on_timer(
        &mut self,
        input: &mut Streaming<api::SubscribeRequest>,
        pending_input: &mut VecDeque<api::SubscribeRequest>,
    ) -> Result<(), Status> {
        if self.challenge.is_some() {
            let mut bytes = 0;
            while let Some(frame) = input.next().now_or_never() {
                let Some(frame) = frame else {
                    return Err(Status::unavailable("request half-closed"));
                };
                let frame = frame?;
                if let Some(api::subscribe_request::Request::Pong(pong)) = &frame.request {
                    self.pong(pong.nonce)?;
                    if self.challenge.is_none() {
                        return Ok(());
                    }
                } else {
                    bytes += frame.encoded_len();
                    if bytes > self.config.limits.max_request_bytes {
                        return Err(Status::resource_exhausted(
                            "pending control capacity exceeded",
                        ));
                    }
                    pending_input.push_back(frame);
                }
            }
            return Err(Status::deadline_exceeded("matching pong not received"));
        }
        self.nonce = self.nonce.wrapping_add(1);
        let (sent, handed) = tokio::sync::oneshot::channel();
        let frame = api::SubscribeResponse {
            response: Some(api::subscribe_response::Response::Ping(api::Ping {
                nonce: self.nonce,
            })),
        };
        let reservation = self
            .mailbox
            .budget
            .reserve(frame.encoded_len() + 5)
            .ok_or_else(capacity)?;
        self.admit(frame, reservation, Some(sent))?;
        self.challenge = Some(Challenge {
            nonce: self.nonce,
            handed,
            deadline: None,
        });
        Ok(())
    }

    fn input(&mut self, frame: api::SubscribeRequest) -> Result<(), Status> {
        match frame
            .request
            .ok_or_else(|| Status::invalid_argument("request is absent"))?
        {
            api::subscribe_request::Request::Update(update) => {
                if !self.updates.take() {
                    return Err(Status::resource_exhausted("update rate exceeded"));
                }
                if self.pending_update.is_some() || !self.deferred_updates.is_empty() {
                    self.deferred_bytes += update.encoded_len();
                    if self.deferred_bytes > self.config.limits.max_request_bytes {
                        return Err(Status::resource_exhausted(
                            "pending update capacity exceeded",
                        ));
                    }
                    self.deferred_updates.push_back(update);
                    Ok(())
                } else {
                    self.update(update)
                }
            }
            api::subscribe_request::Request::Ping(ping) => {
                if !self.pings.take() {
                    return Err(Status::resource_exhausted("ping rate exceeded"));
                }
                self.control(api::subscribe_response::Response::Pong(api::Pong {
                    nonce: ping.nonce,
                }))
            }
            api::subscribe_request::Request::Pong(pong) => self.pong(pong.nonce),
        }
    }

    /// Complete only the matching challenge after its Ping reached transport.
    /// An unrepresentable deadline fails the stream instead of its owner task.
    fn pong(&mut self, nonce: u64) -> Result<(), Status> {
        if let Some(challenge) = &mut self.challenge {
            if challenge.deadline.is_none()
                && let Ok(sent) = challenge.handed.try_recv()
            {
                challenge.start_deadline(
                    sent,
                    Duration::from_millis(self.config.streams.max_pong_wait_ms),
                )?;
            }
            if challenge.nonce == nonce && challenge.deadline.is_some() {
                self.challenge = None;
            }
        }
        Ok(())
    }

    /// Validate the complete update before changing interests. Register all new
    /// topics before starting their head read; no history may precede Applied.
    fn update(&mut self, update: api::subscribe_request::Update) -> Result<(), Status> {
        let adds = self.validate_update(&update)?;
        self.update_id = update.id;
        let mut removed_topics = 0;
        for topic in update.removes {
            removed_topics += usize::from(self.topics.remove(&topic.topic).is_some());
            self.hub.registry.remove(self.id, &topic.topic);
        }
        self.cancel_removed_fetch();
        let mut added = Vec::new();
        for (topic, floor) in adds {
            if self.topics.contains_key(&topic) {
                continue;
            }
            self.hub.registry.add(self.id, topic.clone(), update.id)?;
            self.topics.insert(
                topic.clone(),
                Registration {
                    generation: update.id,
                    floor,
                    needed: floor,
                    queued: false,
                    fetching: false,
                    acknowledged: false,
                },
            );
            added.push(topic);
        }
        let read = self.hub.read.clone();
        let topics = added.clone();
        self.pending_update = Some(PendingUpdate {
            id: update.id,
            removed_topics,
            topics: added,
            heads: async move {
                if topics.is_empty() {
                    return Ok(Vec::new());
                }
                db::stream::heads(&read, &topics)
                    .await
                    .map_err(|_| Status::unavailable("target capture failed"))
            }
            .boxed(),
        });
        Ok(())
    }

    /// Admit targets before enabling new registrations to fetch or become current.
    fn applied(&mut self, heads: Vec<i64>) -> Result<(), Status> {
        let update = self
            .pending_update
            .take()
            .ok_or_else(|| Status::internal("target capture missing"))?;
        let targets = update
            .topics
            .iter()
            .zip(&heads)
            .map(|(topic, &head)| api::CatchupTarget {
                topic: Some(api::Topic {
                    topic: topic.clone(),
                }),
                through_sequence_id: head as u64,
            })
            .collect();
        self.control(api::subscribe_response::Response::Applied(
            api::subscribe_response::Applied {
                id: update.id,
                added_targets: targets,
            },
        ))?;
        tracing::info!(request_id = %self.request_id, update_id = update.id,
            added_topics = update.topics.len(), removed_topics = update.removed_topics,
            "subscription interests updated");
        for (topic, head) in update.topics.into_iter().zip(heads) {
            if let Some(registration) = self.topics.get_mut(&topic) {
                registration.needed = registration.needed.max(head);
                registration.acknowledged = true;
            }
            self.schedule(topic);
        }
        Ok(())
    }

    fn validate_update(
        &self,
        update: &api::subscribe_request::Update,
    ) -> Result<Vec<(Vec<u8>, i64)>, Status> {
        let limits = &self.config.limits;
        if update.id == 0
            || update.id <= self.update_id
            || update.adds.len() > limits.max_update_adds
            || update.removes.len() > limits.max_update_removes
        {
            return Err(Status::invalid_argument("invalid update id or item count"));
        }
        let mut seen = HashSet::new();
        let mut adds = Vec::with_capacity(update.adds.len());
        let mut total = self.topics.len();
        for query in &update.adds {
            let topic = query
                .topic
                .as_ref()
                .ok_or_else(|| Status::invalid_argument("topic is absent"))?;
            Topic::parse(&topic.topic).map_err(|_| Status::invalid_argument("invalid topic"))?;
            let floor = i64::try_from(query.cursor.as_ref().map_or(0, |cursor| cursor.sequence_id))
                .map_err(|_| Status::invalid_argument("invalid cursor"))?;
            if !seen.insert(topic.topic.clone()) {
                return Err(Status::invalid_argument("duplicate update topic"));
            }
            total += usize::from(!self.topics.contains_key(&topic.topic));
            adds.push((topic.topic.clone(), floor));
        }
        for topic in &update.removes {
            Topic::parse(&topic.topic).map_err(|_| Status::invalid_argument("invalid topic"))?;
            if !seen.insert(topic.topic.clone()) {
                return Err(Status::invalid_argument("overlapping update topic"));
            }
            total -= usize::from(self.topics.contains_key(&topic.topic));
        }
        if total > limits.max_stream_topics {
            return Err(Status::invalid_argument("stream topic limit exceeded"));
        }
        Ok(adds)
    }

    /// Keep each active generation once in the FIFO, or switch it atomically
    /// to direct live delivery when its admitted floor covers all known work.
    fn schedule(&mut self, topic: Vec<u8>) {
        let Some(registration) = self.topics.get_mut(&topic) else {
            return;
        };
        if !registration.acknowledged {
            return;
        }
        if registration.floor >= registration.needed {
            registration.needed = self
                .hub
                .registry
                .current(self.id, &topic, registration.floor);
        }
        if registration.floor < registration.needed
            && !registration.queued
            && !registration.fetching
        {
            registration.queued = true;
            self.ready.push_back((topic, registration.generation));
        }
    }

    /// Reserve one bounded data frame before scheduling at most one fair turn.
    fn start_fetch(&mut self) -> Option<PendingFetch> {
        if self.ready.is_empty() {
            return None;
        }
        let budget = self
            .mailbox
            .budget
            .available()
            .min(self.mailbox.frame_bytes);
        if budget < self.config.limits.max_envelope_bytes + ENVELOPE_OVERHEAD {
            return None;
        }
        let reservation = self.mailbox.budget.reserve(budget)?;
        let mut requests = Vec::new();
        while requests.len() < FETCH_TOPICS {
            let Some((topic, generation)) = self.ready.pop_front() else {
                break;
            };
            let Some(registration) = self.topics.get_mut(&topic) else {
                continue;
            };
            if !registration.queued || registration.generation != generation {
                continue;
            }
            registration.queued = false;
            registration.fetching = true;
            requests.push(Request {
                generation: registration.generation,
                range: db::stream::Range {
                    topic,
                    after: registration.floor,
                    through: registration.needed,
                },
            });
        }
        if requests.is_empty() {
            return None;
        }
        Some(PendingFetch {
            page: fetch::fetch(self.hub.clone(), requests.clone(), budget, reservation).boxed(),
            requests,
        })
    }

    /// Cancel a turn that includes a removed registration before acknowledging
    /// removal. Its errors no longer belong to this session. No rows were
    /// admitted, so surviving topics keep their order ahead of later ready work.
    fn cancel_removed_fetch(&mut self) {
        let stale = self.fetching.as_ref().is_some_and(|fetch| {
            fetch.requests.iter().any(|request| {
                self.topics
                    .get(&request.range.topic)
                    .is_none_or(|registration| registration.generation != request.generation)
            })
        });
        if !stale {
            return;
        }
        let fetch = self.fetching.take().expect("stale fetch exists");
        drop(fetch.page);
        for request in fetch.requests.into_iter().rev() {
            if let Some(registration) = self.topics.get_mut(&request.range.topic)
                && registration.generation == request.generation
            {
                registration.fetching = false;
                registration.queued = true;
                self.ready
                    .push_front((request.range.topic, request.generation));
            }
        }
    }

    /// Discard stale generations, advance floors only after output admission,
    /// and preserve unvisited topic priority at a byte cutoff.
    fn fetched(&mut self, page: fetch::ResultPage) -> Result<(), Status> {
        let mut envelopes = Vec::new();
        let mut advances = Vec::new();
        for (ordinal, row) in page.rows {
            let request = &page.requests[ordinal];
            if let Some(registration) = self.topics.get(&row.topic)
                && registration.generation == request.generation
                && row.sequence_id > registration.floor
            {
                advances.push((row.topic.clone(), row.sequence_id));
                envelopes.push(row.try_into()?);
            }
        }
        if !envelopes.is_empty() {
            self.messages(envelopes, page.reservation)?;
            for (topic, floor) in advances {
                if let Some(registration) = self.topics.get_mut(&topic) {
                    registration.floor = floor;
                }
            }
        }
        let mut unvisited = Vec::new();
        for (ordinal, request) in page.requests.into_iter().enumerate() {
            if let Some(registration) = self.topics.get_mut(&request.range.topic)
                && registration.generation == request.generation
            {
                registration.fetching = false;
                if ordinal >= page.visited {
                    registration.queued = true;
                    unvisited.push((request.range.topic, registration.generation));
                } else {
                    self.schedule(request.range.topic);
                }
            }
        }
        for topic in unvisited.into_iter().rev() {
            self.ready.push_front(topic);
        }
        Ok(())
    }

    fn mail(&mut self) -> Result<(), Status> {
        let mail = {
            let mut mail = self.mailbox.mail.lock().expect("mail mutex");
            std::mem::take(&mut *mail)
        };
        for (topic, (generation, head)) in mail.heads {
            if let Some(registration) = self.topics.get_mut(&topic)
                && registration.generation == generation
            {
                registration.needed = registration.needed.max(head);
                self.schedule(topic);
            }
        }
        for batch in mail.live {
            self.live(batch)?;
        }
        Ok(())
    }

    /// Use the shared tailer payload directly. This path never reads payloads
    /// from the database for an already-current registration.
    fn live(&mut self, batch: LiveBatch) -> Result<(), Status> {
        let mut envelopes = Vec::new();
        let mut advances = Vec::new();
        for (generation, row) in batch.rows {
            if let Some(registration) = self.topics.get(&row.topic)
                && registration.generation == generation
                && row.sequence_id > registration.floor
            {
                advances.push((row.topic.clone(), row.sequence_id));
                envelopes.push((*row).clone().try_into()?);
            }
        }
        if envelopes.is_empty() {
            return Ok(());
        }
        self.messages(envelopes, batch.reservation)?;
        for (topic, floor) in advances {
            if let Some(registration) = self.topics.get_mut(&topic) {
                registration.floor = floor;
            }
        }
        Ok(())
    }

    fn messages(
        &mut self,
        envelopes: Vec<api::ServerEnvelope>,
        reservation: Reservation,
    ) -> Result<(), Status> {
        let value = api::SubscribeResponse {
            response: Some(api::subscribe_response::Response::Messages(
                api::subscribe_response::Messages { envelopes },
            )),
        };
        if value.encoded_len() + 5 > self.mailbox.frame_bytes {
            return Err(capacity());
        }
        self.admit(value, reservation, None)
    }

    fn control(&mut self, response: api::subscribe_response::Response) -> Result<(), Status> {
        let value = api::SubscribeResponse {
            response: Some(response),
        };
        let bytes = value.encoded_len() + 5;
        if value.encoded_len() > self.config.limits.max_response_bytes {
            return Err(capacity());
        }
        let reservation = self.mailbox.budget.reserve(bytes).ok_or_else(capacity)?;
        self.admit(value, reservation, None)
    }

    /// Admit one ordered frame. Exhaustion fails the stream; it never drops a
    /// frame while allowing the caller to advance its delivery floor.
    fn admit(
        &mut self,
        value: api::SubscribeResponse,
        mut reservation: Reservation,
        challenge: Option<tokio::sync::oneshot::Sender<Instant>>,
    ) -> Result<(), Status> {
        reservation.shrink(value.encoded_len() + 5);
        self.output
            .try_send(Frame {
                value,
                reservation,
                challenge,
            })
            .map_err(|_| capacity())?;
        self.send_idle = Instant::now();
        Ok(())
    }
}

fn capacity() -> Status {
    Status::resource_exhausted("stream output capacity exceeded")
}

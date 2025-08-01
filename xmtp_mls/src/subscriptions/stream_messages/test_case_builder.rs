use std::collections::HashMap;
use std::future::{self, Future, Ready};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::subscriptions::process_message::ProcessedMessage;
use crate::test::mock::{MockContext, MockProcessFutureFactory, NewMockContext};
use crate::test::mock::{context, generate_message, generate_message_and_v1, generate_stored_msg};
use mockall::Sequence;
use parking_lot::Mutex;
use pin_project_lite::pin_project;
use xmtp_api::test_utils::MockGroupStream;
use xmtp_common::FutureWrapper;
use xmtp_common::types::GroupId;

use xmtp_proto::mls_v1::QueryGroupMessagesResponse;

pin_project! {
    pub struct ReadyAfter<Fut> {
        #[pin] future: Fut,
        after: usize,
        polled_so_far: usize
    }
}

impl<Fut> Future for ReadyAfter<Fut>
where
    Fut: Future,
{
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.polled_so_far < this.after {
            *this.polled_so_far += 1;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        this.future.poll(cx)
    }
}

/// create a future that will be immediately ready after 'after' times, returning T
pub fn ready_after<T>(t: T, after: usize) -> ReadyAfter<Ready<T>> {
    ReadyAfter {
        future: future::ready(t),
        after,
        polled_so_far: 0,
    }
}

#[derive(Clone, Debug)]
pub struct StreamSession {
    // groups to call add() with
    pub groups: Vec<GroupTestCase>,
    // messages coming after the add
    pub messages: Vec<MessageCase>,
    // array of expected cursors
    pub expected: Vec<u64>,
}

pub fn group_list_from_session(sessions: &[StreamSession]) -> Vec<GroupId> {
    sessions
        .iter()
        .next()
        .expect("sessions must have at least one init")
        .groups
        .iter()
        .copied()
        .map(|g| group_id(g.group_id))
        .collect()
}

impl StreamSession {
    pub fn messages(&self) -> Vec<MessageCase> {
        self.messages.clone()
    }

    pub fn session(groups: Vec<u8>, messages: Vec<MessageCase>, expected: Vec<u64>) -> Self {
        StreamSession {
            groups: groups
                .into_iter()
                .map(|g| GroupTestCase { group_id: g })
                .collect(),
            messages,
            expected,
        }
    }
}

pub fn group_id(id: u8) -> GroupId {
    let mut v = vec![id];
    v.resize(31, 0);
    GroupId::from(v)
}

/*
        /// Message will be processed, and it will be busy for 'busy_for' poll_next calls
        fn busy_for(cursor: u64, group_id: u8, next_cursor: u64, busy_for: u8) -> Self {
            Self::message(cursor, group_id, true, next_cursor, false, busy_for)
        }
*/

#[derive(Default)]
pub struct CaseState {
    pub current_session: usize,
    pub sessions: HashMap<usize, StreamSession>,
}

fn setup_stream(cases: Vec<MessageCase>, stream: &mut MockGroupStream) {
    let mut msg_seq = Sequence::new();
    for case in &cases {
        if case.polls_to_resolve > 1 {
            stream
                .expect_poll_next()
                .times((case.polls_to_resolve - 1) as usize)
                .in_sequence(&mut msg_seq)
                .returning(move |_| {
                    tracing::info!("item is pending");
                    Poll::Pending
                });
        }
        stream
            .expect_poll_next()
            .once()
            .in_sequence(&mut msg_seq)
            .returning({
                let case = *case;
                move |_| {
                    let (msg, _) = generate_message_and_v1(case.cursor, &group_id(case.group_id));
                    Poll::Ready(Some(Ok(msg)))
                }
            });
    }
    // default value for a stream is to just end it
    // doesn't need to be called necessarily
    stream.expect_poll_next().returning(|_| Poll::Ready(None));
}

#[derive(Clone, Debug, Copy)]
pub struct MessageCase {
    pub cursor: u64,
    pub group_id: u8,
    pub found: bool,
    pub next_cursor: u64,
    /// whether the message is retrieved from the db
    pub retrieved: bool,
    /// amnt of polls that return pending before future resolved
    pub polls_to_resolve: u8,
    pub polls_to_process: u8,
}

impl MessageCase {
    fn message(
        cursor: u64,
        group_id: u8,
        found: bool,
        next_cursor: u64,
        retrieved: bool,
        polls_to_resolve: u8,
        polls_to_process: u8,
    ) -> Self {
        MessageCase {
            cursor,
            group_id,
            found,
            next_cursor,
            retrieved,
            polls_to_resolve,
            polls_to_process,
        }
    }

    pub fn found(cursor: u64, group_id: u8, next_cursor: u64) -> Self {
        Self::message(cursor, group_id, true, next_cursor, false, 1, 1)
    }

    /// The message should be retrieved from the database
    pub fn retrieved(cursor: u64, group_id: u8, found: bool) -> Self {
        Self::message(cursor, group_id, found, 9999, true, 1, 1)
    }

    pub fn not_found(cursor: u64, group_id: u8, next_cursor: u64) -> Self {
        Self::message(cursor, group_id, false, next_cursor, false, 1, 1)
    }

    /// Number of polls that a message will be processing for before resolved
    pub fn processing_for(cursor: u64, group_id: u8, next_cursor: u64, processing_for: u8) -> Self {
        Self::message(
            cursor,
            group_id,
            true,
            next_cursor,
            false,
            1,
            processing_for,
        )
    }
}

#[derive(Clone, Debug, Copy, Default)]
pub struct GroupTestCase {
    pub group_id: u8,
}

pub struct StreamSequenceBuilder {
    session_counter: usize,
    sessions: HashMap<usize, StreamSession>,
    factory: MockProcessFutureFactory,
    context: NewMockContext,
    case_state: Arc<Mutex<CaseState>>,
    process_sequence: Sequence,
}

impl Default for StreamSequenceBuilder {
    fn default() -> Self {
        Self {
            session_counter: Default::default(),
            sessions: Default::default(),
            factory: Default::default(),
            context: context(),
            case_state: Default::default(),
            process_sequence: Default::default(),
        }
    }
}

pub struct FinishedSequence {
    pub context: MockContext,
    // dont want to drop them
    #[allow(unused)]
    pub case_state: Arc<Mutex<CaseState>>,
    #[allow(unused)]
    pub process_sequence: Sequence,
}

impl StreamSequenceBuilder {
    // internal api to add session to sequence and case state
    fn add_session(&mut self, session: StreamSession) {
        self.sessions.insert(self.session_counter, session.clone());
        self.case_state
            .lock()
            .sessions
            .insert(self.session_counter, session.clone());
        self.session_counter += 1;
    }

    pub fn session(&mut self, session: StreamSession) {
        if self.sessions.is_empty() {
            self.add_session(session.clone());
            self.init_session(session.groups.clone())
        } else {
            self.add_session(session.clone());
            self.create_session(session.groups.clone());
        }
        self.init_processing(session);
    }

    pub fn init_processing(&mut self, session: StreamSession) {
        let messages = session.messages();
        for case in &messages {
            if !case.retrieved {
                self.factory
                    .expect_create()
                    .once()
                    .in_sequence(&mut self.process_sequence)
                    .returning({
                        let case = *case;
                        move |msg| {
                            FutureWrapper::new(ready_after(
                                Ok(ProcessedMessage {
                                    message: case
                                        .found
                                        .then(|| generate_stored_msg(msg.id, msg.group_id.clone())),
                                    group_id: msg.group_id,
                                    next_message: case.next_cursor,
                                    tried_to_process: msg.id,
                                }),
                                case.polls_to_process.into(),
                            ))
                        }
                    });
            } else {
                self.factory
                    .expect_retrieve()
                    .once()
                    .in_sequence(&mut self.process_sequence)
                    .returning({
                        let case = *case;
                        move |msg| {
                            Ok(case
                                .found
                                .then(|| generate_stored_msg(msg.id, msg.group_id.clone())))
                        }
                    });
            }
        }
    }

    fn init_session(&mut self, groups: Vec<GroupTestCase>) {
        // Set up db mock expectation
        let db_calls = || {
            let mut mock_db = xmtp_db::mock::MockDbQuery::new();
            // Set up expectations for commonly called db methods
            mock_db
                .expect_get_latest_sequence_id_for_group()
                .returning(|_| Ok(None));
            mock_db
                .expect_get_latest_sequence_id()
                .returning(|_| Ok(HashMap::new()));
            mock_db
        };
        self.context.store.expect_db().returning(db_calls);

        let times = groups.len();
        self.context
            .api_client
            .api_client
            .expect_query_group_messages()
            .times(times)
            .returning(|req| {
                let message = generate_message(1, &req.group_id);
                Ok(QueryGroupMessagesResponse {
                    messages: vec![message],
                    paging_info: None,
                })
            });
        let state = self.case_state.clone();
        self.context
            .api_client
            .api_client
            .expect_subscribe_group_messages()
            .once()
            .returning(move |_req| {
                let mut state = state.lock();
                let session = state.sessions.get(&state.current_session).unwrap();
                let mut mock = MockGroupStream::new();
                setup_stream(session.messages(), &mut mock);
                state.current_session += 1;
                Ok(mock)
            });
    }

    fn create_session(&mut self, groups: Vec<GroupTestCase>) {
        let times = groups.len();

        // Set up expectations for query_group_messages for the new groups
        self.context
            .api_client
            .api_client
            .expect_query_group_messages()
            .times(times)
            .returning(|req| {
                let message = generate_message(1, &req.group_id);
                Ok(QueryGroupMessagesResponse {
                    messages: vec![message],
                    paging_info: None,
                })
            });

        self.context
            .api_client
            .api_client
            .expect_subscribe_group_messages()
            .times(times - 1)
            .returning(|_req| Ok(MockGroupStream::new()));
        let state = self.case_state.clone();
        self.context
            .api_client
            .api_client
            .expect_subscribe_group_messages()
            .once()
            .returning(move |_req| {
                let mut state = state.lock();
                let session = state.sessions.get(&state.current_session).unwrap();
                let mut mock = MockGroupStream::new();
                setup_stream(session.messages(), &mut mock);
                state.current_session += 1;
                Ok(mock)
            });
    }

    pub fn finish(self) -> (MockProcessFutureFactory, FinishedSequence) {
        (
            self.factory,
            FinishedSequence {
                context: Arc::new(self.context),
                case_state: self.case_state,
                process_sequence: self.process_sequence,
            },
        )
    }
}

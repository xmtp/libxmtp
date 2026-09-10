use std::sync::Arc;

use napi::bindgen_prelude::{
  BigInt, Error, FnArgs, Function, Result, Uint8Array, within_runtime_if_available,
};
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use tokio::sync::Mutex;
use xmtp_mls::{
  MlsContext,
  context::XmtpSharedContext,
  subscriptions::{
    incoming::{
      IncomingConnection, IncomingCoordinator, IncomingProcessing, IncomingRegistration,
      IncomingStatus,
    },
    local_delivery::{
      DeliveryAcknowledgement, DeliveryCursor as RustDeliveryCursor, DeliveryScope, LocalDelivery,
      LocalDeliveryError, LocalDeliveryFilter, LocalDeliveryItem,
    },
    message_reader::{MessageReader as RustMessageReader, MessageReaderControl},
  },
};

use crate::{
  ErrorWrapper,
  consent_state::ConsentState,
  conversation::Conversation,
  conversations::{ConversationType, Conversations},
  messages::Message,
  streams::StreamCloser,
};

/// An exclusive replay position in one database, independent of network progress.
#[napi(object)]
pub struct DeliveryCursor {
  /// Stable across reopen; whole-database restore invalidates earlier identities.
  pub database_id: Uint8Array,
  /// Immutable local order. Zero precedes the first retained message.
  pub delivery_sequence: BigInt,
}

impl From<RustDeliveryCursor> for DeliveryCursor {
  fn from(value: RustDeliveryCursor) -> Self {
    Self {
      database_id: value.database_id.to_vec().into(),
      delivery_sequence: value.delivery_sequence.into(),
    }
  }
}

impl TryFrom<DeliveryCursor> for RustDeliveryCursor {
  type Error = Error;
  fn try_from(value: DeliveryCursor) -> Result<Self> {
    let (signed, sequence, lossless) = value.delivery_sequence.get_u64();
    if signed || !lossless {
      return Err(
        ErrorWrapper::from(xmtp_db::stream_storage::StreamStorageError::InvalidDeliveryPosition)
          .into(),
      );
    }
    let database_id = value.database_id.as_ref().try_into().map_err(|_| {
      Error::from(ErrorWrapper::from(
        xmtp_db::stream_storage::StreamStorageError::ForeignCursor,
      ))
    })?;
    Ok(Self {
      database_id,
      delivery_sequence: sequence,
    })
  }
}

/// Opaque acknowledgement token. Queue insertion alone must not consume the message.
#[napi]
pub struct MessageAcknowledgement {
  inner: Arc<DeliveryAcknowledgement<MlsContext>>,
}

#[napi]
impl MessageAcknowledgement {
  /// Call immediately before the app callback or iterator handoff. False means reselect.
  #[napi]
  pub fn check_owner(&self) -> Result<bool> {
    match self.inner.check_owner() {
      Ok(()) => Ok(true),
      Err(LocalDeliveryError::SelectionChanged) => Ok(false),
      Err(error) => Err(ErrorWrapper::from(error).into()),
    }
  }

  #[napi]
  /// Persist D only after successful callback completion or the next iterator request.
  pub fn acknowledge(&self) -> Result<()> {
    self
      .inner
      .acknowledge()
      .map_err(|error| ErrorWrapper::from(error).into())
  }

  #[napi]
  /// Reject a current handoff without advancing D. A stale selection is discarded.
  pub fn reject(&self) {
    self.inner.reject();
  }
}

/// One message with its cursor and an acknowledgement that stays pending until host completion.
#[napi]
pub struct MessageDelivery {
  message: Message,
  cursor: RustDeliveryCursor,
  acknowledgement: Arc<DeliveryAcknowledgement<MlsContext>>,
}

impl From<LocalDeliveryItem<MlsContext>> for MessageDelivery {
  fn from(value: LocalDeliveryItem<MlsContext>) -> Self {
    Self {
      message: value.message.into(),
      cursor: value.cursor,
      acknowledgement: Arc::new(value.acknowledgement),
    }
  }
}

#[napi]
impl MessageDelivery {
  #[napi(getter)]
  pub fn message(&self) -> Message {
    self.message.clone()
  }
  #[napi(getter)]
  pub fn cursor(&self) -> DeliveryCursor {
    self.cursor.into()
  }
  #[napi(getter)]
  pub fn acknowledgement(&self) -> MessageAcknowledgement {
    MessageAcknowledgement {
      inner: Arc::clone(&self.acknowledgement),
    }
  }
}

/// Fixed-target receipt and processing status for one topic.
#[napi(object)]
pub struct MessageTopicStatus {
  /// Wire topic bytes identify the obligation independently of its current scope.
  pub topic: Uint8Array,
  /// Scope generation that registered this topic.
  pub scope_generation: BigInt,
  #[napi(ts_type = "'Pending' | 'Active' | 'Removed'")]
  pub registration: String,
  /// Captured H, or absent while target capture is still pending.
  pub target: Option<BigInt>,
  /// F: durably admitted network progress, not application acknowledgement.
  pub received: BigInt,
  /// P: committed processing progress.
  pub processed: BigInt,
  /// Unresolved actual Welcome rows through H; missing integer IDs do not count.
  pub unresolved_welcomes: BigInt,
  #[napi(ts_type = "'Pending' | 'Complete' | 'Blocked' | 'Cancelled'")]
  pub processing: String,
  pub error_code: Option<String>,
}

/// One connection and scope generation, separate from application acknowledgement.
#[napi(object)]
pub struct MessageCatchUpGeneration {
  /// Changes when the set of requested topics changes.
  pub scope_generation: BigInt,
  /// Changes when the receiver reconnects; it does not reset application progress.
  pub connection_generation: BigInt,
  #[napi(ts_type = "'Connecting' | 'Connected' | 'Reconnecting' | 'Failed' | 'Closed'")]
  pub connection: String,
  pub topics: Vec<MessageTopicStatus>,
  /// True while Welcome-driven group discovery for this generation is incomplete.
  pub discovery_pending: bool,
  #[napi(ts_type = "'Pending' | 'Complete' | 'Blocked' | 'Cancelled'")]
  pub processing: String,
  pub error_code: Option<String>,
}

/// Current and previous catch-up generations; a snapshot does not consume messages.
#[napi(object)]
pub struct MessageCatchUp {
  /// Current scope and connection generation; independent of default delivery D.
  pub current: MessageCatchUpGeneration,
  /// Previous generation retained so a reconnect does not hide its outcome.
  pub previous: Option<MessageCatchUpGeneration>,
}

fn connection_name(value: IncomingConnection) -> &'static str {
  match value {
    IncomingConnection::Connecting => "Connecting",
    IncomingConnection::Connected => "Connected",
    IncomingConnection::Reconnecting => "Reconnecting",
    IncomingConnection::Failed => "Failed",
    IncomingConnection::Closed => "Closed",
  }
}

fn processing_name(value: IncomingProcessing) -> &'static str {
  match value {
    IncomingProcessing::Pending => "Pending",
    IncomingProcessing::Complete => "Complete",
    IncomingProcessing::Blocked => "Blocked",
    IncomingProcessing::Cancelled => "Cancelled",
  }
}

fn registration_name(value: IncomingRegistration) -> &'static str {
  match value {
    IncomingRegistration::Pending => "Pending",
    IncomingRegistration::Active => "Active",
    IncomingRegistration::Removed => "Removed",
  }
}

impl From<IncomingStatus> for MessageCatchUp {
  fn from(mut value: IncomingStatus) -> Self {
    let previous = value.previous.take().map(|previous| (*previous).into());
    Self {
      current: value.into(),
      previous,
    }
  }
}

impl From<IncomingStatus> for MessageCatchUpGeneration {
  fn from(value: IncomingStatus) -> Self {
    Self {
      scope_generation: value.scope_generation.into(),
      connection_generation: value.connection_generation.into(),
      connection: connection_name(value.connection).into(),
      discovery_pending: value.discovery_pending,
      processing: processing_name(value.processing).into(),
      error_code: value.error.as_ref().map(|error| error.code().to_string()),
      topics: value
        .topics
        .into_iter()
        .map(|topic| MessageTopicStatus {
          topic: topic.topic.cloned_vec().into(),
          scope_generation: topic.scope_generation.into(),
          registration: registration_name(topic.registration).into(),
          target: topic.target.map(|target| target.0.into()),
          received: topic.received.0.into(),
          processed: topic.processed.0.into(),
          unresolved_welcomes: topic.unresolved_welcomes.into(),
          processing: processing_name(topic.processing).into(),
          error_code: topic
            .blocked
            .or_else(|| topic.error.as_ref().map(|error| error.code().to_string())),
        })
        .collect(),
    }
  }
}

/// A retained history message with its immutable local delivery cursor.
#[napi(object)]
pub struct MessageWithCursor {
  pub message: Message,
  /// Cursor assigned when this message first became locally deliverable.
  pub cursor: DeliveryCursor,
}

/// History and its resume cursor captured in one database read snapshot.
#[napi(object)]
pub struct MessageHistorySnapshot {
  /// Retained history selected in the same database snapshot as the cursor.
  pub messages: Vec<MessageWithCursor>,
  /// Start replay after this position to receive messages stored after the snapshot.
  pub cursor: DeliveryCursor,
}

impl From<xmtp_db::delivery::DeliverySnapshot> for MessageHistorySnapshot {
  fn from(value: xmtp_db::delivery::DeliverySnapshot) -> Self {
    Self {
      messages: value
        .messages
        .into_iter()
        .map(|item| MessageWithCursor {
          message: item.message.into(),
          cursor: item.cursor.into(),
        })
        .collect(),
      cursor: value.cursor.into(),
    }
  }
}

/// The database's sole default consumer, or independent cursor replay, with one pending token.
#[napi]
pub struct MessageReader {
  inner: Arc<Mutex<RustMessageReader<MlsContext>>>,
  control: MessageReaderControl,
}

impl MessageReader {
  pub(crate) fn new(
    context: MlsContext,
    scope: DeliveryScope,
    filter: LocalDeliveryFilter,
    from: Option<DeliveryCursor>,
  ) -> Result<Self> {
    let _coordinator = IncomingCoordinator::enable_bidi_transport(&context);
    let reader = RustMessageReader::new(
      context,
      scope,
      filter,
      from.map(TryInto::try_into).transpose()?,
    )
    .map_err(ErrorWrapper::from)?;
    let control = reader.control();
    Ok(Self {
      inner: Arc::new(Mutex::new(reader)),
      control,
    })
  }
}

#[napi]
impl MessageReader {
  /// Returns one unacknowledged item. A second call waits for its token.
  #[napi]
  pub async fn next_delivery(&self) -> Result<Option<MessageDelivery>> {
    self
      .inner
      .lock()
      .await
      .next_delivery()
      .await
      .map(|item| item.map(Into::into))
      .map_err(|error| ErrorWrapper::from(error).into())
  }
  #[napi]
  /// Release the default owner and stop this reader without acknowledging its pending item.
  pub fn close(&self) {
    self.control.close();
  }
  #[napi]
  /// Replace the group scope. Queued items outside the new scope must be selected again.
  pub fn update_scope(&self, group_ids: Option<Vec<String>>) -> Result<()> {
    self.control.update_scope(parse_scope(group_ids)?);
    Ok(())
  }
  #[napi]
  /// Replace future selection filters; an executing callback keeps its existing token.
  pub fn update_filter(
    &self,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) {
    self
      .control
      .update_filter(filter(conversation_type, consent_states));
  }
  #[napi]
  /// Read network catch-up state without reading or advancing delivery D.
  pub fn catch_up_snapshot(&self) -> MessageCatchUp {
    self.control.catch_up_snapshot().into()
  }
  #[napi]
  /// Wait for a catch-up state change or reader close, then return its latest snapshot.
  pub async fn catch_up_changed(&self) -> MessageCatchUp {
    self.control.changed().await;
    self.control.catch_up_snapshot().into()
  }
}

impl Drop for MessageReader {
  fn drop(&mut self) {
    self.control.close();
  }
}

fn filter(
  conversation_type: Option<ConversationType>,
  consent_states: Option<Vec<ConsentState>>,
) -> LocalDeliveryFilter {
  LocalDeliveryFilter {
    conversation_type: conversation_type.map(Into::into),
    consent_states: consent_states.map(|states| states.into_iter().map(Into::into).collect()),
  }
}

pub(crate) struct QueuedMessage {
  message: Message,
  acknowledgement: Arc<DeliveryAcknowledgement<MlsContext>>,
}

struct CallbackClose(ThreadsafeFunction<(), ()>);

impl Drop for CallbackClose {
  fn drop(&mut self) {
    self.0.call(Ok(()), ThreadsafeFunctionCallMode::NonBlocking);
  }
}

#[allow(
  clippy::type_complexity,
  reason = "Keep the callback type equal to the NAPI exports"
)]
pub(crate) fn callback_stream(
  context: MlsContext,
  scope: DeliveryScope,
  selected_filter: LocalDeliveryFilter,
  callback: Function<'_, FnArgs<(Option<Error>, Option<Message>)>, ()>,
  on_close: ThreadsafeFunction<(), ()>,
) -> Result<StreamCloser> {
  // This conversion runs on the JavaScript thread, immediately before its callback.
  let callback = callback
    .build_threadsafe_function::<QueuedMessage>()
    .callee_handled::<false>()
    .max_queue_size::<1>()
    .build_callback(|queued| match queued.value.acknowledgement.check_owner() {
      Ok(()) => Ok(FnArgs::from((None::<Error>, Some(queued.value.message)))),
      Err(error) => Ok(FnArgs::from((
        Some(Error::from(ErrorWrapper::from(error))),
        None::<Message>,
      ))),
    })?;
  // The synchronous export runs on the JavaScript thread, outside Tokio.
  within_runtime_if_available(|| {
    let _coordinator = IncomingCoordinator::enable_bidi_transport(&context);
    let mut reader =
      RustMessageReader::new(context, scope, selected_filter, None).map_err(ErrorWrapper::from)?;
    let control = reader.control();
    let on_close = CallbackClose(on_close);
    let handle = xmtp_common::spawn(None, async move {
      let _on_close = on_close;
      let result = async {
        while let Some(item) = reader.next_delivery().await? {
          let acknowledgement = Arc::new(item.acknowledgement);
          let returned = callback
            .call_async_catch(QueuedMessage {
              message: item.message.into(),
              acknowledgement: Arc::clone(&acknowledgement),
            })
            .await;
          match acknowledgement.check_owner() {
            Err(LocalDeliveryError::SelectionChanged) => continue,
            Err(error) => return Err(error),
            Ok(()) => {}
          }
          if returned.is_err() {
            acknowledgement.reject();
            return Err(LocalDeliveryError::AcknowledgementRejected);
          }
          acknowledgement.acknowledge()?;
        }
        Ok::<_, LocalDeliveryError>(())
      }
      .await;
      result.map_err(xmtp_mls::subscriptions::SubscribeError::from)
    });
    Ok(StreamCloser::new_message(handle, control))
  })
}

fn parse_scope(group_ids: Option<Vec<String>>) -> Result<DeliveryScope> {
  match group_ids {
    None => Ok(DeliveryScope::All),
    Some(ids) => Ok(DeliveryScope::Groups(
      ids
        .into_iter()
        .map(|id| {
          let bytes = hex::decode(id).map_err(|error| Error::from_reason(error.to_string()))?;
          xmtp_proto::types::GroupId::try_from(bytes)
            .map_err(|error| ErrorWrapper::from(error).into())
        })
        .collect::<Result<Vec<_>>>()?,
    )),
  }
}

#[napi]
impl Conversations {
  /// Return a database-bound cursor before the first retained message.
  #[napi]
  pub fn beginning_delivery_cursor(&self) -> Result<DeliveryCursor> {
    use xmtp_db::delivery::QueryDelivery;
    Ok(
      RustDeliveryCursor {
        database_id: self
          .inner_client
          .context
          .db()
          .stream_database_id()
          .map_err(ErrorWrapper::from)?,
        delivery_sequence: 0,
      }
      .into(),
    )
  }
  /// Open the database's sole default consumer, or independent replay after a cursor.
  #[napi]
  pub async fn message_reader(
    &self,
    group_ids: Option<Vec<String>>,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
    from: Option<DeliveryCursor>,
  ) -> Result<MessageReader> {
    MessageReader::new(
      self.inner_client.context.clone(),
      parse_scope(group_ids)?,
      filter(conversation_type, consent_states),
      from,
    )
  }
  /// Read retained history and its resume cursor from one database snapshot.
  #[napi]
  pub fn message_history_snapshot(
    &self,
    limit: u32,
    group_ids: Option<Vec<String>>,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<MessageHistorySnapshot> {
    LocalDelivery::history_snapshot(
      &self.inner_client.context,
      &parse_scope(group_ids)?,
      &filter(conversation_type, consent_states),
      limit,
    )
    .map(Into::into)
    .map_err(|error| ErrorWrapper::from(error).into())
  }
}

#[napi]
impl Conversation {
  /// Return the database-bound start cursor for replay in this conversation.
  #[napi]
  pub fn beginning_delivery_cursor(&self) -> Result<DeliveryCursor> {
    use xmtp_db::delivery::QueryDelivery;
    Ok(
      RustDeliveryCursor {
        database_id: self
          .create_mls_group()
          .context
          .db()
          .stream_database_id()
          .map_err(ErrorWrapper::from)?,
        delivery_sequence: 0,
      }
      .into(),
    )
  }
  /// Open a default consumer scoped to this group, or independent replay after the cursor.
  #[napi]
  pub async fn message_reader(&self, from: Option<DeliveryCursor>) -> Result<MessageReader> {
    let group = self.create_mls_group();
    MessageReader::new(
      group.context.clone(),
      DeliveryScope::Groups(vec![group.group_id]),
      LocalDeliveryFilter::default(),
      from,
    )
  }
  /// Read this group's history and resume cursor in the same database snapshot.
  #[napi]
  pub fn message_history_snapshot(&self, limit: u32) -> Result<MessageHistorySnapshot> {
    let group = self.create_mls_group();
    LocalDelivery::history_snapshot(
      &group.context,
      &DeliveryScope::Groups(vec![group.group_id]),
      &LocalDeliveryFilter::default(),
      limit,
    )
    .map(Into::into)
    .map_err(|error| ErrorWrapper::from(error).into())
  }
}

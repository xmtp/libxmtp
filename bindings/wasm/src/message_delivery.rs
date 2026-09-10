use std::{rc::Rc, sync::Arc};

use futures::lock::Mutex;
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;
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
  streams::{StreamCallback, StreamCloser},
};

/// An exclusive replay position in one database, independent of network progress.
#[derive(Clone, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryCursor {
  /// Stable across reopen; whole-database restore invalidates earlier identities.
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub database_id: Vec<u8>,
  /// Immutable local order. Zero precedes the first retained message.
  pub delivery_sequence: u64,
}

impl From<RustDeliveryCursor> for DeliveryCursor {
  fn from(value: RustDeliveryCursor) -> Self {
    Self {
      database_id: value.database_id.to_vec(),
      delivery_sequence: value.delivery_sequence,
    }
  }
}

impl TryFrom<DeliveryCursor> for RustDeliveryCursor {
  type Error = JsError;
  fn try_from(value: DeliveryCursor) -> Result<Self, JsError> {
    let database_id = value
      .database_id
      .try_into()
      .map_err(|_| ErrorWrapper::js(xmtp_db::stream_storage::StreamStorageError::ForeignCursor))?;
    Ok(Self {
      database_id,
      delivery_sequence: value.delivery_sequence,
    })
  }
}

/// Opaque acknowledgement token. Queue insertion alone must not consume the message.
#[wasm_bindgen]
pub struct MessageAcknowledgement {
  inner: Arc<DeliveryAcknowledgement<MlsContext>>,
}

#[wasm_bindgen]
impl MessageAcknowledgement {
  /// Call immediately before the app callback or iterator handoff. False means reselect.
  #[wasm_bindgen(js_name = checkOwner)]
  pub fn check_owner(&self) -> Result<bool, JsError> {
    match self.inner.check_owner() {
      Ok(()) => Ok(true),
      Err(LocalDeliveryError::SelectionChanged) => Ok(false),
      Err(error) => Err(ErrorWrapper::js(error)),
    }
  }
  /// Persist D only after successful callback completion or the next iterator request.
  pub fn acknowledge(&self) -> Result<(), JsError> {
    self.inner.acknowledge().map_err(ErrorWrapper::js)
  }
  /// Reject a current handoff without advancing D. A stale selection is discarded.
  pub fn reject(&self) {
    self.inner.reject();
  }
}

/// One message with its cursor and an acknowledgement that stays pending until host completion.
#[wasm_bindgen]
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

#[wasm_bindgen]
impl MessageDelivery {
  #[wasm_bindgen(getter)]
  pub fn message(&self) -> Message {
    self.message.clone()
  }
  #[wasm_bindgen(getter)]
  pub fn cursor(&self) -> DeliveryCursor {
    self.cursor.into()
  }
  #[wasm_bindgen(getter)]
  pub fn acknowledgement(&self) -> MessageAcknowledgement {
    MessageAcknowledgement {
      inner: Arc::clone(&self.acknowledgement),
    }
  }
}

/// Fixed-target receipt and processing status for one topic.
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct MessageTopicStatus {
  /// Wire topic bytes identify the obligation independently of its current scope.
  #[serde(with = "serde_bytes")]
  #[tsify(type = "Uint8Array")]
  pub topic: Vec<u8>,
  /// Scope generation that registered this topic.
  pub scope_generation: u64,
  #[tsify(type = "'Pending' | 'Active' | 'Removed'")]
  pub registration: String,
  /// Captured H, or absent while target capture is still pending.
  pub target: Option<u64>,
  /// F: durably admitted network progress, not application acknowledgement.
  pub received: u64,
  /// P: committed processing progress.
  pub processed: u64,
  /// Unresolved actual Welcome rows through H; missing integer IDs do not count.
  pub unresolved_welcomes: u64,
  #[tsify(type = "'Pending' | 'Complete' | 'Blocked' | 'Cancelled'")]
  pub processing: String,
  pub error_code: Option<String>,
}

/// One connection and scope generation, separate from application acknowledgement.
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct MessageCatchUpGeneration {
  /// Changes when the set of requested topics changes.
  pub scope_generation: u64,
  /// Changes when the receiver reconnects; it does not reset application progress.
  pub connection_generation: u64,
  #[tsify(type = "'Connecting' | 'Connected' | 'Reconnecting' | 'Failed' | 'Closed'")]
  pub connection: String,
  pub topics: Vec<MessageTopicStatus>,
  /// True while Welcome-driven group discovery for this generation is incomplete.
  pub discovery_pending: bool,
  #[tsify(type = "'Pending' | 'Complete' | 'Blocked' | 'Cancelled'")]
  pub processing: String,
  pub error_code: Option<String>,
}

/// Current and previous catch-up generations; a snapshot does not consume messages.
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
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
      scope_generation: value.scope_generation,
      connection_generation: value.connection_generation,
      connection: connection_name(value.connection).into(),
      discovery_pending: value.discovery_pending,
      processing: processing_name(value.processing).into(),
      error_code: value.error.as_ref().map(|error| error.code().to_string()),
      topics: value
        .topics
        .into_iter()
        .map(|topic| MessageTopicStatus {
          topic: topic.topic.cloned_vec(),
          scope_generation: topic.scope_generation,
          registration: registration_name(topic.registration).into(),
          target: topic.target.map(|target| target.0),
          received: topic.received.0,
          processed: topic.processed.0,
          unresolved_welcomes: topic.unresolved_welcomes,
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
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
pub struct MessageWithCursor {
  pub message: Message,
  /// Cursor assigned when this message first became locally deliverable.
  pub cursor: DeliveryCursor,
}

/// History and its resume cursor captured in one database read snapshot.
#[derive(Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi, large_number_types_as_bigints)]
#[serde(rename_all = "camelCase")]
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
#[wasm_bindgen]
pub struct MessageReader {
  inner: Rc<Mutex<RustMessageReader<MlsContext>>>,
  control: MessageReaderControl,
}

impl MessageReader {
  pub(crate) fn new(
    context: MlsContext,
    scope: DeliveryScope,
    filter: LocalDeliveryFilter,
    from: Option<DeliveryCursor>,
  ) -> Result<Self, JsError> {
    let _coordinator = IncomingCoordinator::enable_stream_transport(&context);
    let reader = RustMessageReader::new(
      context,
      scope,
      filter,
      from.map(TryInto::try_into).transpose()?,
    )
    .map_err(ErrorWrapper::js)?;
    let control = reader.control();
    Ok(Self {
      inner: Rc::new(Mutex::new(reader)),
      control,
    })
  }
}

#[wasm_bindgen]
impl MessageReader {
  /// Returns one unacknowledged item. A second call waits for its token.
  #[wasm_bindgen(js_name = nextDelivery)]
  pub async fn next_delivery(&self) -> Result<Option<MessageDelivery>, JsError> {
    self
      .inner
      .lock()
      .await
      .next_delivery()
      .await
      .map(|item| item.map(Into::into))
      .map_err(ErrorWrapper::js)
  }
  /// Release the default owner and stop this reader without acknowledging its pending item.
  pub fn close(&self) {
    self.control.close();
  }
  #[wasm_bindgen(js_name = updateScope)]
  /// Replace the group scope. Queued items outside the new scope must be selected again.
  pub fn update_scope(&self, group_ids: Option<Vec<String>>) -> Result<(), JsError> {
    self.control.update_scope(parse_scope(group_ids)?);
    Ok(())
  }
  #[wasm_bindgen(js_name = updateFilter)]
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
  #[wasm_bindgen(js_name = catchUpSnapshot)]
  /// Read network catch-up state without reading or advancing delivery D.
  pub fn catch_up_snapshot(&self) -> MessageCatchUp {
    self.control.catch_up_snapshot().into()
  }
  #[wasm_bindgen(js_name = catchUpChanged)]
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

struct CallbackClose(StreamCallback);

impl Drop for CallbackClose {
  fn drop(&mut self) {
    let _ = self.0.on_close_caught();
  }
}

pub(crate) fn callback_stream(
  context: MlsContext,
  scope: DeliveryScope,
  filter: LocalDeliveryFilter,
  callback: StreamCallback,
) -> Result<StreamCloser, JsError> {
  let _coordinator = IncomingCoordinator::enable_stream_transport(&context);
  let mut reader =
    RustMessageReader::new(context, scope, filter, None).map_err(ErrorWrapper::js)?;
  let control = reader.control();
  let on_close = CallbackClose(callback.clone());
  let handle = xmtp_common::spawn(None, async move {
    let _on_close = on_close;
    let result = async {
      while let Some(item) = reader.next_delivery().await? {
        match item.acknowledgement.check_owner() {
          Err(LocalDeliveryError::SelectionChanged) => continue,
          Err(error) => return Err(error),
          Ok(()) => {}
        }
        if callback.on_message(item.message.into()).is_err() {
          item.acknowledgement.reject();
          return Err(LocalDeliveryError::AcknowledgementRejected);
        }
        item.acknowledgement.acknowledge()?;
      }
      Ok::<_, LocalDeliveryError>(())
    }
    .await;
    if let Err(error) = &result {
      let _ = callback.on_error_caught(crate::errors::error_to_js(error));
    }
    result.map_err(xmtp_mls::subscriptions::SubscribeError::from)
  });
  Ok(StreamCloser::new_message(handle, control))
}

fn parse_scope(group_ids: Option<Vec<String>>) -> Result<DeliveryScope, JsError> {
  match group_ids {
    None => Ok(DeliveryScope::All),
    Some(ids) => Ok(DeliveryScope::Groups(
      ids
        .into_iter()
        .map(|id| {
          let bytes = hex::decode(id).map_err(|error| JsError::new(&error.to_string()))?;
          xmtp_proto::types::GroupId::try_from(bytes).map_err(ErrorWrapper::js)
        })
        .collect::<Result<Vec<_>, _>>()?,
    )),
  }
}

#[wasm_bindgen]
impl Conversations {
  /// Return a database-bound cursor before the first retained message.
  #[wasm_bindgen(js_name = beginningDeliveryCursor)]
  pub fn beginning_delivery_cursor(&self) -> Result<DeliveryCursor, JsError> {
    use xmtp_db::delivery::QueryDelivery;
    Ok(
      RustDeliveryCursor {
        database_id: self
          .inner_client
          .context
          .db()
          .stream_database_id()
          .map_err(ErrorWrapper::js)?,
        delivery_sequence: 0,
      }
      .into(),
    )
  }
  /// Open the database's sole default consumer, or independent replay after a cursor.
  #[wasm_bindgen(js_name = messageReader)]
  pub fn message_reader(
    &self,
    group_ids: Option<Vec<String>>,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
    from: Option<DeliveryCursor>,
  ) -> Result<MessageReader, JsError> {
    MessageReader::new(
      self.inner_client.context.clone(),
      parse_scope(group_ids)?,
      filter(conversation_type, consent_states),
      from,
    )
  }
  /// Read retained history and its resume cursor from one database snapshot.
  #[wasm_bindgen(js_name = messageHistorySnapshot)]
  pub fn message_history_snapshot(
    &self,
    limit: u32,
    group_ids: Option<Vec<String>>,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<MessageHistorySnapshot, JsError> {
    LocalDelivery::history_snapshot(
      &self.inner_client.context,
      &parse_scope(group_ids)?,
      &filter(conversation_type, consent_states),
      limit,
    )
    .map(Into::into)
    .map_err(ErrorWrapper::js)
  }
}

#[wasm_bindgen]
impl Conversation {
  /// Return the database-bound start cursor for replay in this conversation.
  #[wasm_bindgen(js_name = beginningDeliveryCursor)]
  pub fn beginning_delivery_cursor(&self) -> Result<DeliveryCursor, JsError> {
    use xmtp_db::delivery::QueryDelivery;
    Ok(
      RustDeliveryCursor {
        database_id: self
          .to_mls_group()
          .context
          .db()
          .stream_database_id()
          .map_err(ErrorWrapper::js)?,
        delivery_sequence: 0,
      }
      .into(),
    )
  }
  /// Open a default consumer scoped to this group, or independent replay after the cursor.
  #[wasm_bindgen(js_name = messageReader)]
  pub fn message_reader(&self, from: Option<DeliveryCursor>) -> Result<MessageReader, JsError> {
    let group = self.to_mls_group();
    MessageReader::new(
      group.context.clone(),
      DeliveryScope::Groups(vec![group.group_id]),
      LocalDeliveryFilter::default(),
      from,
    )
  }
  /// Read this group's history and resume cursor in the same database snapshot.
  #[wasm_bindgen(js_name = messageHistorySnapshot)]
  pub fn message_history_snapshot(&self, limit: u32) -> Result<MessageHistorySnapshot, JsError> {
    let group = self.to_mls_group();
    LocalDelivery::history_snapshot(
      &group.context,
      &DeliveryScope::Groups(vec![group.group_id]),
      &LocalDeliveryFilter::default(),
      limit,
    )
    .map(Into::into)
    .map_err(ErrorWrapper::js)
  }
}

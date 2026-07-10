use crate::ErrorWrapper;
use crate::consent_state::{Consent, ConsentState};
use crate::conversation::Conversation;
use crate::conversations::{ConversationType, Conversations};
use crate::messages::Message;
use crate::messages::decoded_message::DecodedMessage;
use crate::{client::RustXmtpClient, streams::StreamCloser};
use napi::bindgen_prelude::{Error, Result, Uint8Array};
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use xmtp_db::consent_record::ConsentState as XmtpConsentState;
use xmtp_mls::subscriptions::router_callbacks::bidi_streams_enabled;
use xmtp_mls::worker::device_sync::preference_sync::PreferenceUpdate as XmtpUserPreferenceUpdate;

#[napi(discriminant = "type")]
pub enum UserPreferenceUpdate {
  ConsentUpdate { consent: Consent },
  HmacKeyUpdate { key: Uint8Array },
}

impl From<XmtpUserPreferenceUpdate> for UserPreferenceUpdate {
  fn from(value: XmtpUserPreferenceUpdate) -> Self {
    match value {
      XmtpUserPreferenceUpdate::Hmac { key, .. } => Self::HmacKeyUpdate { key: key.into() },
      XmtpUserPreferenceUpdate::Consent(consent) => Self::ConsentUpdate {
        consent: consent.into(),
      },
    }
  }
}

#[napi]
impl Conversations {
  #[napi]
  #[xmtp_common::err_span]
  pub async fn stream(
    &self,
    callback: ThreadsafeFunction<Conversation, ()>,
    on_close: ThreadsafeFunction<(), ()>,
    conversation_type: Option<ConversationType>,
  ) -> Result<StreamCloser> {
    let on_conversation =
      move |convo: std::result::Result<_, xmtp_mls::subscriptions::SubscribeError>| {
        let status = callback.call(
          convo
            .map(Conversation::from)
            .map_err(ErrorWrapper::from)
            .map_err(Error::from),
          ThreadsafeFunctionCallMode::Blocking,
        );
        tracing::info!("Stream status: {:?}", status);
      };
    let on_close = move || {
      on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
    };

    if bidi_streams_enabled() {
      // Interim bidi scope — locally-created conversations do not surface on
      // this stream yet: see `stream_conversations_with_callback_bidi`'s docs.
      let handle = RustXmtpClient::stream_conversations_with_callback_bidi(
        self.inner_client.clone(),
        conversation_type.map(|ct| ct.into()),
        false,
        on_conversation,
        on_close,
      );
      Ok(StreamCloser::new(handle))
    } else {
      let handle = RustXmtpClient::stream_conversations_with_callback(
        self.inner_client.clone(),
        conversation_type.map(|ct| ct.into()),
        on_conversation,
        on_close,
        false,
      );
      Ok(StreamCloser::new(handle))
    }
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn stream_all_messages(
    &self,
    callback: ThreadsafeFunction<Message, ()>,
    on_close: ThreadsafeFunction<(), ()>,
    conversation_type: Option<ConversationType>,
    consent_states: Option<Vec<ConsentState>>,
  ) -> Result<StreamCloser> {
    tracing::trace!(
      inbox_id = self.inner_client.inbox_id(),
      conversation_type = ?conversation_type,
    );

    let inbox_id = self.inner_client.inbox_id().to_string();
    let consents: Option<Vec<XmtpConsentState>> = consent_states.map(|states| {
      states
        .into_iter()
        .map(|state: ConsentState| state.into())
        .collect()
    });

    let on_message = move |message| {
      tracing::trace!(
          inbox_id,
          conversation_type = ?conversation_type,
          "[received] message result"
      );

      if let Err(error) = &message {
        tracing::warn!(
          inbox_id,
          error = ?error,
          "[received] forwarding message error to callback"
        );
      }

      let status = callback.call(
        message
          .map(Into::into)
          .map_err(ErrorWrapper::from)
          .map_err(Error::from),
        ThreadsafeFunctionCallMode::Blocking,
      );
      tracing::info!("Stream status: {:?}", status);
    };
    let on_close = move || {
      on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
    };

    if bidi_streams_enabled() {
      // Interim bidi scope — covers the groups known at subscribe time: see
      // `stream_all_messages_with_callback_bidi`'s docs.
      let handle = RustXmtpClient::stream_all_messages_with_callback_bidi(
        self.inner_client.clone(),
        conversation_type.map(Into::into),
        consents,
        on_message,
        on_close,
      );
      Ok(StreamCloser::new(handle))
    } else {
      let handle = RustXmtpClient::stream_all_messages_with_callback(
        self.inner_client.context.clone(),
        conversation_type.map(Into::into),
        consents,
        on_message,
        on_close,
      );
      Ok(StreamCloser::new(handle))
    }
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn stream_consent(
    &self,
    callback: ThreadsafeFunction<Vec<Consent>, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id(),);
    let inbox_id = self.inner_client.inbox_id().to_string();
    let stream_closer = RustXmtpClient::stream_consent_with_callback(
      self.inner_client.clone(),
      move |message| {
        tracing::trace!(inbox_id, "[received] calling tsfn callback");
        match message {
          Ok(message) => {
            let msg: Vec<Consent> = message.into_iter().map(Into::into).collect();
            let status = callback.call(Ok(msg), ThreadsafeFunctionCallMode::Blocking);
            tracing::info!("Stream status: {:?}", status);
          }
          Err(e) => {
            let status = callback.call(
              Err(Error::from(ErrorWrapper::from(e))),
              ThreadsafeFunctionCallMode::Blocking,
            );
            tracing::info!("Stream status: {:?}", status);
          }
        }
      },
      move || {
        on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn stream_preferences(
    &self,
    callback: ThreadsafeFunction<Vec<UserPreferenceUpdate>, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id());
    let inbox_id = self.inner_client.inbox_id().to_string();
    let stream_closer = RustXmtpClient::stream_preferences_with_callback(
      self.inner_client.clone(),
      move |message| {
        tracing::trace!(inbox_id, "[received] calling tsfn callback");
        match message {
          Ok(message) => {
            let msg: Vec<UserPreferenceUpdate> = message
              .into_iter()
              .map(UserPreferenceUpdate::from)
              .collect();
            let status = callback.call(Ok(msg), ThreadsafeFunctionCallMode::Blocking);
            tracing::info!("Stream status: {:?}", status);
          }
          Err(e) => {
            let status = callback.call(
              Err(Error::from(ErrorWrapper::from(e))),
              ThreadsafeFunctionCallMode::Blocking,
            );
            tracing::info!("Stream status: {:?}", status);
          }
        }
      },
      move || {
        let status = on_close.call(Ok(()), ThreadsafeFunctionCallMode::Blocking);
        tracing::info!("stream on close status {:?}", status);
      },
    );

    Ok(StreamCloser::new(stream_closer))
  }

  #[napi]
  #[xmtp_common::err_span]
  pub async fn stream_message_deletions(
    &self,
    callback: ThreadsafeFunction<DecodedMessage, ()>,
  ) -> Result<StreamCloser> {
    tracing::trace!(inbox_id = self.inner_client.inbox_id());
    let stream_closer = RustXmtpClient::stream_message_deletions_with_callback(
      self.inner_client.clone(),
      move |message| match message {
        Ok(decoded_message) => match DecodedMessage::try_from(decoded_message) {
          Ok(msg) => {
            let _ = callback.call(Ok(msg), ThreadsafeFunctionCallMode::Blocking);
          }
          Err(e) => {
            let _ = callback.call(Err(e), ThreadsafeFunctionCallMode::Blocking);
          }
        },
        Err(e) => {
          let _ = callback.call(
            Err(Error::from(ErrorWrapper::from(e))),
            ThreadsafeFunctionCallMode::Blocking,
          );
        }
      },
      || {},
    );

    Ok(StreamCloser::new(stream_closer))
  }
}

use crate::{conversation::Conversation, messages::Message, streams::StreamCloser};
use napi::{
  bindgen_prelude::{Error, FnArgs, Function, Result},
  threadsafe_function::ThreadsafeFunction,
};
use napi_derive::napi;
use xmtp_mls::subscriptions::local_delivery::{DeliveryScope, LocalDeliveryFilter};

#[napi]
impl Conversation {
  #[napi]
  #[xmtp_common::err_span]
  #[allow(
    clippy::type_complexity,
    reason = "NAPI needs the full callback type for TypeScript generation"
  )]
  pub fn stream(
    &self,
    callback: Function<'_, FnArgs<(Option<Error>, Option<Message>)>, ()>,
    on_close: ThreadsafeFunction<(), ()>,
  ) -> Result<StreamCloser> {
    let group = self.create_mls_group();
    crate::message_delivery::callback_stream(
      group.context.clone(),
      DeliveryScope::Groups(vec![group.group_id]),
      LocalDeliveryFilter::default(),
      callback,
      on_close,
    )
  }
}

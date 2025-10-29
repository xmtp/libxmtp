//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

mod client;
use std::{any::Any, error::Error, sync::Arc};

pub use client::*;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::ApiClientError;

use crate::{
    ToDynApi,
    definitions::FullD14nClient,
    protocol::{CursorStore, FullXmtpApiT},
};

//TODO:temp_cache_workaround
pub fn d14n_new_with_store<E>(
    api: Arc<dyn FullXmtpApiT<E>>,
    store: Arc<dyn CursorStore>,
) -> Option<Arc<dyn FullXmtpApiT<ApiClientError<GrpcError>>>>
where
    E: Error + RetryableError + Send + Sync + 'static,
{
    let new: Arc<dyn Any + Send + Sync> = api;
    if let Ok(c) = new.downcast::<FullD14nClient>() {
        // create a new allocation that doesn't point to the given store
        let mut c = Arc::unwrap_or_clone(c);
        c.cursor_store = store;
        Some(c.arced())
    } else {
        None
    }
}

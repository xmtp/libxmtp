//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

mod client;
use std::{error::Error, sync::Arc};

use crate::definitions::FullD14nClient;
use crate::definitions::FullV3Client;
use crate::protocol::AnyClient;
pub use client::*;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::ApiClientError;

use crate::{
    ToDynApi,
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
    // create a new type that doesn't point to the given store
    if let Some(c) = api.as_ref().downcast_ref_d14nclient() {
        // ensure we clone but not in an Arc<>
        let mut c: D14nClient<_, _, _> = c.clone();
        c.cursor_store = store;
        Some(c.arced())
    } else {
        None
    }
}

impl AnyClient for FullD14nClient {
    fn downcast_ref_v3client(&self) -> Option<&'_ FullV3Client> {
        None
    }

    fn downcast_ref_d14nclient(&self) -> Option<&'_ FullD14nClient> {
        Some(self)
    }
}

//! Compatibility layer for d14n and previous xmtp_api crate
mod client;
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;
xmtp_common::if_test! {
    mod test_client;
}
pub use client::*;

use crate::definitions::FullD14nClient;
use crate::definitions::FullV3Client;
use crate::protocol::AnyClient;
use crate::{
    ToDynApi,
    protocol::{CursorStore, FullXmtpApiT},
};
use std::sync::Arc;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::ApiClientError;

//TODO:temp_cache_workaround
pub fn d14n_new_with_store<E>(
    api: Arc<dyn FullXmtpApiT<E>>,
    store: Arc<dyn CursorStore>,
) -> Option<Arc<dyn FullXmtpApiT<ApiClientError<GrpcError>>>>
where
    E: RetryableError + 'static,
{
    // create a new type that doesn't point to the given store
    if let Some(c) = api.as_ref().downcast_ref_d14nclient() {
        // ensure we clone but not in an Arc<>
        let mut c: D14nClient<_, _> = c.clone();
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

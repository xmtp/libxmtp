mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

mod client;
use std::sync::Arc;

use crate::definitions::FullV3Client;
use crate::protocol::AnyClient;
use crate::{
    ToDynApi,
    protocol::{CursorStore, FullXmtpApiT},
};
pub use client::*;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::ApiClientError;

//TODO:temp_cache_workaround
pub fn v3_new_with_store<E>(
    api: Arc<dyn FullXmtpApiT<E>>,
    store: Arc<dyn CursorStore>,
) -> Option<Arc<dyn FullXmtpApiT<ApiClientError<GrpcError>>>>
where
    E: RetryableError + 'static,
{
    // create a new type that doesn't point to the given store
    if let Some(c) = api.as_ref().downcast_ref_v3client() {
        Some(V3Client::new(c.client.clone(), store).arced())
    } else {
        None
    }
}

impl AnyClient for FullV3Client {
    fn downcast_ref_v3client(&self) -> Option<&'_ FullV3Client> {
        Some(self)
    }
}

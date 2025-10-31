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
use std::{error::Error, sync::Arc};
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::ApiClientError;

//TODO:temp_cache_workaround
pub fn v3_new_with_store<E>(
    api: Arc<dyn FullXmtpApiT<E>>,
    store: Arc<dyn CursorStore>,
) -> Option<Arc<dyn FullXmtpApiT<ApiClientError<GrpcError>>>>
where
    E: Error + RetryableError + Send + Sync + 'static,
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

    fn downcast_ref_d14nclient(&self) -> Option<&'_ FullD14nClient> {
        None
    }
}

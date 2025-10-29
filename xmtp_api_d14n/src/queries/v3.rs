mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

mod client;
use std::{any::Any, sync::Arc};

pub use client::*;
use xmtp_api_grpc::error::GrpcError;
use xmtp_common::RetryableError;
use xmtp_proto::api::{Client, IsConnectedCheck};

use crate::{
    ToDynApi,
    protocol::{CursorStore, FullXmtpApiT, NoCursorStore},
};

pub fn new_with_store(
    api: Arc<dyn FullXmtpApiT>,
    store: Arc<dyn CursorStore>,
) -> Option<Arc<dyn FullXmtpApiT>> {
    let new: Arc<dyn Any + Send + Sync> = api;
    if let Ok(c) = new.downcast::<V3Client<_, _>>() {
        let mut c = c.clone();
        c.cursor_store = store;
        Some(c.arced())
    } else {
        None
    }
}

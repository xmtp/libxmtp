//! Compatibility layer for d14n and previous xmtp_api crate
mod identity;
mod mls;
mod streams;
mod to_dyn_api;
mod xmtp_query;

mod client;
use std::{any::Any, sync::Arc};

pub use client::*;
use xmtp_api_grpc::GrpcClient;
use xmtp_common::RetryableError;

use crate::{
    ToDynApi,
    protocol::{CursorStore, FullXmtpApiT, NoCursorStore},
};

pub fn new_with_store(
    api: Arc<dyn FullXmtpApiT>,
    store: Arc<dyn CursorStore>,
) -> Result<Option<Arc<dyn FullXmtpApiT>>, Box<dyn RetryableError>> {
    let new: Arc<dyn Any + Send + Sync> = api;
    if let Ok(c) = new.downcast::<D14nClient<_, _, _>>() {
        let mut c = c.clone();
        c.cursor_store = store;
        Ok(Some(c.arced()))
    } else {
        Ok(None)
    }
}

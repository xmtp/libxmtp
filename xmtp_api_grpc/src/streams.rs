mod escapable;
use std::{future::Future, pin::Pin};

pub use escapable::*;

mod default;
pub use default::*;

mod non_blocking_request;
pub use non_blocking_request::*;

mod non_blocking_stream;
pub use non_blocking_stream::*;

use prost::bytes::Bytes;
use tonic::{Response, Status, Streaming};

type Stream = Streaming<Bytes>;

pub(crate) type NonBlocking =
    EscapableTonicStream<NonBlockingWebStream<NonBlockingStreamRequest<ResponseFuture>, Stream>>;

pub type XmtpStream<T> = XmtpTonicStream<crate::GrpcStream, T>;

xmtp_common::if_wasm! {
    type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Stream>, Status>>>>;
}

xmtp_common::if_native! {
    type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Stream>, Status>> + Send>>;
}

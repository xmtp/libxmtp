mod escapable;
use std::{future::Future, pin::Pin};

pub use escapable::*;

mod default;
pub use default::*;

mod non_blocking_request;
pub use non_blocking_request::*;

mod non_blocking_stream;
pub use non_blocking_stream::*;

mod try_from_item;
pub use try_from_item::*;

mod fake_empty;
pub use fake_empty::*;

mod multiplexed;
pub use multiplexed::*;

use prost::bytes::Bytes;
use tonic::{Response, Status, Streaming};

type Stream = Streaming<Bytes>;

pub(crate) type NonBlocking =
    EscapableTonicStream<NonBlockingWebStream<NonBlockingStreamRequest<ResponseFuture>, Stream>>;

/// Web and Native compatible network stream of Protobuf types from an XMTP Backend.
pub type XmtpStream<T> = XmtpTonicStream<crate::GrpcStream, T>;

xmtp_common::if_wasm! {
    pub type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Stream>, Status>>>>;
}

xmtp_common::if_native! {
    pub type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Stream>, Status>> + Send>>;
}

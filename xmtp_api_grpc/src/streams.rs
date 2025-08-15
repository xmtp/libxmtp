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
#[cfg(target_arch = "wasm32")]
type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Stream>, Status>>>>;

#[cfg(not(target_arch = "wasm32"))]
type ResponseFuture = Pin<Box<dyn Future<Output = Result<Response<Stream>, Status>> + Send>>;

pub(crate) type NonBlocking = EscapableTonicStream<
    NonBlockingWebStream<NonBlockingStreamRequest<ResponseFuture>, Stream>,
>;

pub type XmtpStream<T> = XmtpTonicStream<crate::GrpcStream, T>;

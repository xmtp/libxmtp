mod escapable;
use std::{future::Future, pin::Pin};

pub use escapable::*;

mod web_compat;
pub use web_compat::*;

mod default;
pub use default::*;

use tonic::{Response, Status, Streaming};

type ResponseFuture<T> = Pin<Box<dyn Future<Output = Result<Response<Streaming<T>>, Status>> + Send + Sync + Unpin>>;

type NonBlocking<'a, T> =
    NonBlockingWebStream<'a, ResponseFuture<T>, EscapableTonicStream<Streaming<T>, T>, T>;

pub type XmtpStream<'a, T> = XmtpTonicStream<NonBlocking<'a, T>>;

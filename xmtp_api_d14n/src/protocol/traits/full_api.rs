use std::{any::Any, sync::Arc};

use xmtp_api_grpc::error::GrpcError;
use xmtp_proto::{
    api::{ApiClientError, IsConnectedCheck},
    api_client::{BoxedGroupS, BoxedWelcomeS},
    prelude::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
};

use crate::protocol::XmtpQuery;

/// A type-erased version of the Xmtp Api in a [`Box`]
pub type FullXmtpApiBox = Box<dyn FullXmtpApiT>;
/// A type-erased version of the Xntp Api in a [`Arc`]
pub type FullXmtpApiArc = Arc<dyn FullXmtpApiT>;

// TODO: Can remove boxes once switchover to d14n (one client) is complete.

type ErrorType = ApiClientError<GrpcError>;

/// Trait combining all other api traits into one
/// Used for describing the entire XmtpApi from
/// the client perspective in a single `dyn Trait`
/// or otherwise requiring the full capabilities
/// of the API.
/// Requiring the full capabilities outside of a dyn should generally be avoided
/// unless the consumer wants to be unnecessarily general/restrictive.
pub trait FullXmtpApiT
where
    Self: Any
        + XmtpMlsClient<Error = ErrorType>
        + XmtpIdentityClient<Error = ErrorType>
        + XmtpMlsStreams<
            Error = ErrorType,
            WelcomeMessageStream = BoxedWelcomeS<ErrorType>,
            GroupMessageStream = BoxedGroupS<ErrorType>,
        > + IsConnectedCheck
        + XmtpQuery<Error = ErrorType>
        + Send
        + Sync
        + 'static,
{
}

impl<T> FullXmtpApiT for T where
    T: Any
        + XmtpMlsClient<Error = ErrorType>
        + XmtpIdentityClient<Error = ErrorType>
        + XmtpMlsStreams<
            Error = ErrorType,
            WelcomeMessageStream = BoxedWelcomeS<ErrorType>,
            GroupMessageStream = BoxedGroupS<ErrorType>,
        > + IsConnectedCheck
        + XmtpQuery<Error = ErrorType>
        + Send
        + Sync
        + ?Sized
        + 'static
{
}

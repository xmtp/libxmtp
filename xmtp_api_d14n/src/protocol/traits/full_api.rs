use std::{any::Any, sync::Arc};

use xmtp_proto::{
    api::IsConnectedCheck,
    api_client::{BoxedGroupS, BoxedWelcomeS},
    prelude::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
};

use crate::{
    definitions::{FullD14nClient, FullV3Client},
    protocol::XmtpQuery,
};

/// A type-erased version of the Xmtp Api in a [`Box`]
pub type FullXmtpApiBox<Err> = Box<dyn FullXmtpApiT<Err>>;
/// A type-erased version of the Xntp Api in a [`Arc`]
pub type FullXmtpApiArc<Err> = Arc<dyn FullXmtpApiT<Err>>;

// TODO: Can remove boxes once switchover to d14n (one client) is complete.

/// Trait combining all other api traits into one
/// Used for describing the entire XmtpApi from
/// the client perspective in a single `dyn Trait`
/// or otherwise requiring the full capabilities
/// of the API.
/// Requiring the full capabilities outside of a dyn should generally be avoided
/// unless the consumer wants to be unnecessarily general/restrictive.
pub trait FullXmtpApiT<Err>
where
    Self: AnyClient
        + XmtpMlsClient<Error = Err>
        + XmtpIdentityClient<Error = Err>
        + XmtpMlsStreams<
            Error = Err,
            WelcomeMessageStream = BoxedWelcomeS<Err>,
            GroupMessageStream = BoxedGroupS<Err>,
        > + IsConnectedCheck
        + XmtpQuery<Error = Err>
        + Send
        + Sync
        + 'static,
{
}

impl<T, Err> FullXmtpApiT<Err> for T where
    T: AnyClient
        + XmtpMlsClient<Error = Err>
        + XmtpIdentityClient<Error = Err>
        + XmtpMlsStreams<
            Error = Err,
            WelcomeMessageStream = BoxedWelcomeS<Err>,
            GroupMessageStream = BoxedGroupS<Err>,
        > + IsConnectedCheck
        + XmtpQuery<Error = Err>
        + Send
        + Sync
        + ?Sized
        + 'static
{
}

// TEMP trait
pub trait AnyClient: Any + 'static {
    fn downcast_ref_v3client(&self) -> Option<&'_ FullV3Client>;
    fn downcast_ref_d14nclient(&self) -> Option<&'_ FullD14nClient>;
}

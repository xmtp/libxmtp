use std::sync::Arc;

use xmtp_proto::{
    api::IsConnectedCheck,
    api_client::{BoxedGroupS, BoxedWelcomeS},
    prelude::{XmtpIdentityClient, XmtpMlsClient, XmtpMlsStreams},
};

// The XIP-83 bidi requirement, folded into the full API only where it can
// exist: `Send`-style Maybe* bridge — on native the full API must speak
// bidi (with the boxed stream, so it stays `dyn`-compatible); on wasm the
// bound is vacuous (gRPC-Web cannot full-duplex).
xmtp_common::if_native! {
    pub trait MaybeBidiStreams<Err>:
        xmtp_proto::api_client::XmtpMlsBidiStreams<
            Error = Err,
            SubscribeStream = xmtp_proto::api_client::BoxedSubscribeS<Err>,
        >
    {
    }
    impl<T, Err> MaybeBidiStreams<Err> for T where
        T: xmtp_proto::api_client::XmtpMlsBidiStreams<
                Error = Err,
                SubscribeStream = xmtp_proto::api_client::BoxedSubscribeS<Err>,
            > + ?Sized
    {
    }
}
xmtp_common::if_wasm! {
    pub trait MaybeBidiStreams<Err> {}
    impl<T, Err> MaybeBidiStreams<Err> for T where T: ?Sized {}
}

use crate::protocol::XmtpQuery;

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
    Self: XmtpMlsClient<Error = Err>
        + XmtpIdentityClient<Error = Err>
        + XmtpMlsStreams<
            Error = Err,
            WelcomeMessageStream = BoxedWelcomeS<Err>,
            GroupMessageStream = BoxedGroupS<Err>,
        > + IsConnectedCheck
        + XmtpQuery<Error = Err>
        + MaybeBidiStreams<Err>
        + 'static,
{
}

impl<T, Err> FullXmtpApiT<Err> for T where
    T: XmtpMlsClient<Error = Err>
        + XmtpIdentityClient<Error = Err>
        + XmtpMlsStreams<
            Error = Err,
            WelcomeMessageStream = BoxedWelcomeS<Err>,
            GroupMessageStream = BoxedGroupS<Err>,
        > + IsConnectedCheck
        + XmtpQuery<Error = Err>
        + MaybeBidiStreams<Err>
        + ?Sized
        + 'static
{
}

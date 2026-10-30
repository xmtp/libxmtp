//! The charge decision for every RPC the backend serves (RATE section 2).

use crate::config::Category::{self, *};

/// What one RPC path is charged.
pub(crate) enum Charges {
    /// Never charged, by any category.
    Exempt,
    /// The layer spends 1 `request` unit; each other category listed is spent
    /// by the one step of the handler or stream that does the work.
    Charged(&'static [Category]),
}

/// Every method of every service the backend serves. The layer treats a path
/// missing from this table as one no service implements and charges nothing,
/// so a new RPC needs a row here before it is served.
pub(crate) static CHARGES: &[(&str, Charges)] = &[
    ("/grpc.health.v1.Health/Check", Charges::Exempt),
    ("/grpc.health.v1.Health/Watch", Charges::Exempt),
    (
        "/xmtp.backend.v1.ConfigurationService/GetConfiguration",
        Charges::Exempt,
    ),
    (
        "/xmtp.backend.v1.AttachmentService/CreateUpload",
        Charges::Charged(&[Request, Upload]),
    ),
    (
        "/xmtp.backend.v1.QueryService/Query",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.QueryService/QueryNewest",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.PublishService/Publish",
        Charges::Charged(&[Request, IdentityUpdate, ChainVerify, Publish]),
    ),
    (
        "/xmtp.backend.v1.SubscriptionService/Subscribe",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.SubscriptionService/SubscribeStatic",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.IdentityService/GetInboxIds",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.IdentityService/VerifySmartContractWalletSignatures",
        Charges::Charged(&[Request, ChainVerify]),
    ),
    (
        "/xmtp.backend.v1.NotificationService/Register",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.NotificationService/Unregister",
        Charges::Charged(&[Request]),
    ),
    (
        "/xmtp.backend.v1.NotificationService/UpdateSubscriptions",
        Charges::Charged(&[Request, Push]),
    ),
];

/// The categories a request on `path` is charged, or `None` when the path is
/// exempt or no service implements it. Tonic routes only exact method paths,
/// so an exact match covers every request a handler receives.
pub(super) fn charged(path: &str) -> Option<&'static [Category]> {
    CHARGES
        .iter()
        .find(|(route, _)| *route == path)
        .and_then(|(_, charges)| match charges {
            Charges::Exempt => None,
            Charges::Charged(categories) => Some(*categories),
        })
}

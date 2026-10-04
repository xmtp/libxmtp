//! The core error variants marked `#[error(transparent)]` that the error walk
//! opens. Such a variant forwards `source()` to its inner error's source, so a
//! walk over `source()` alone skips the inner error and its typed cause.
//! Variants with their own message keep the inner error as their source and
//! need no entry. `tests::transparent_wrappers` checks that every transparent
//! variant of these enums is opened here or allowed with a reason.

use std::error::Error;

macro_rules! transparent_wrappers {
    ($($ty:ty => [$($variant:ident $(($boxed:ident))?),* $(,)?]),* $(,)?) => {
        /// The opened variants as (enum, variant) names, for the guard test.
        #[cfg(all(test, not(target_arch = "wasm32"), not(feature = "pure-only")))]
        pub(crate) const OPENED_WRAPPERS: &[(&str, &str)] = &[
            $($((stringify!($ty), stringify!($variant)),)*)*
        ];

        /// The inner error of an opened transparent wrapper variant.
        pub(super) fn wrapped<'a>(
            error: &'a (dyn Error + 'static),
        ) -> Option<&'a (dyn Error + 'static)> {
            $({
                type Wrapper = $ty;
                if let Some(error) = error.downcast_ref::<Wrapper>() {
                    #[allow(unreachable_patterns)]
                    return match error {
                        $(Wrapper::$variant(inner) => {
                            Some(transparent_wrappers!(@open inner $($boxed)?))
                        })*
                        _ => None,
                    };
                }
            })*
            None
        }
    };
    (@open $inner:ident boxed) => { &**$inner };
    (@open $inner:ident) => { $inner };
}

transparent_wrappers! {
    xmtp_mls::client::ClientError => [IdentityUpdate, Group(boxed), Db, MlsStore],
    xmtp_mls::groups::GroupError => [
        StreamBarrier, ProcessIntent, Db, MlsStore, Diesel, DeviceSync(boxed),
    ],
    xmtp_mls::builder::ClientBuilderError => [
        StorageLocation, ClientError, Identity, WrappedApiError, GroupError(boxed),
        DeviceSync(boxed),
    ],
    xmtp_mls::identity::IdentityError => [StorageError, ApiClient, IdentityUpdate, Db],
    xmtp_mls::identity_updates::IdentityUpdateError => [Api, Load(boxed)],
    xmtp_mls::identity_updates::InstallationDiffError => [IdentityDependency, Client, Db, Storage],
    xmtp_mls::identity_updates::IdentityDependencyError => [Client(boxed), Storage, Shared(boxed)],
    xmtp_mls::groups::validated_commit::CommitValidationError => [
        IdentityDependency, InstallationDiff, StorageError,
    ],
    xmtp_mls::mls_store::MlsStoreError => [Storage, Api, Connection],
    xmtp_mls::groups::intents::IntentError => [Storage],
    xmtp_mls::groups::GroupMessageProcessingError => [
        Identity, Intent, ProcessIntent, Client, Db, Diesel,
    ],
    xmtp_mls::subscriptions::incoming::IncomingError => [
        Storage, Store, Transport, Group, Processing, Identity,
    ],
    xmtp_mls::subscriptions::local_delivery::LocalDeliveryError => [
        SessionFailure(boxed), Configuration(boxed), Storage,
    ],
    xmtp_mls::worker::device_sync::DeviceSyncError => [Db, MlsStore, Subscribe, Archive],
    xmtp_mls::subscriptions::SubscribeError => [
        LocalDelivery, Group(boxed), Storage, ApiClient, Db, Configuration(boxed),
    ],
    xmtp_archive::ArchiveError => [Storage],
    xmtp_mls::subscriptions::catch_up::CatchUpError => [Group],
}

/// A core error type that `XmtpError::from_core` maps. Every type passed to
/// `from_core` must implement it, so a new call-site type must be listed here,
/// and the guard test starts its scan from this list (`CORE_ERROR_ROOTS`).
pub(crate) trait CoreError: Error + 'static {}

macro_rules! core_errors {
    ($($(#[$meta:meta])* $ty:ty),* $(,)?) => {
        $($(#[$meta])* impl CoreError for $ty {})*

        /// The type names that implement `CoreError`, for the guard test.
        #[cfg(all(test, not(target_arch = "wasm32"), not(feature = "pure-only")))]
        pub(crate) const CORE_ERROR_ROOTS: &[&str] = &[$(stringify!($ty)),*];
    };
}

// Wrapper types: the guard follows their variants.
core_errors! {
    xmtp_mls::groups::GroupError,
    xmtp_mls::client::ClientError,
    xmtp_mls::builder::ClientBuilderError,
    xmtp_mls::identity::IdentityError,
    xmtp_mls::mls_store::MlsStoreError,
    xmtp_mls::subscriptions::SubscribeError,
    xmtp_mls::subscriptions::catch_up::CatchUpError,
    xmtp_mls::subscriptions::local_delivery::LocalDeliveryError,
    xmtp_mls::worker::device_sync::DeviceSyncError,
    xmtp_mls::messages::enrichment::EnrichMessageError,
    xmtp_archive::ArchiveError,
    // Classified at their own level.
    xmtp_db::StorageError,
    xmtp_db::ConnectionError,
    xmtp_db::sql_key_store::SqlKeyStoreError,
    #[cfg(target_arch = "wasm32")]
    xmtp_db::PlatformStorageError,
    // Leaf errors with no typed cause in the error table; the guard checks
    // that none of them hides a classified type.
    xmtp_mls::mls_common::app_data::component_source::ComponentSourceError,
    xmtp_mls::mls_common::group_metadata::GroupMetadataError,
    xmtp_id::associations::AssociationError,
    xmtp_cryptography::signature::IdentifierValidationError,
    xmtp_api_backend::MessageBackendBuilderError,
    xmtp_common::StreamHandleError,
    xmtp_content_types::CodecError,
    xmtp_proto::ConversionError,
    xmtp_logging::Error,
    prost::DecodeError,
    std::io::Error,
    #[cfg(not(target_arch = "wasm32"))]
    tokio::task::JoinError,
}

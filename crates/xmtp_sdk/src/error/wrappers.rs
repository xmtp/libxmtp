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
        #[cfg(all(test, not(feature = "pure-only")))]
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
        ClientError, Identity, WrappedApiError, GroupError(boxed), DeviceSync(boxed),
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
    xmtp_mls::worker::device_sync::DeviceSyncError => [Db, MlsStore],
}

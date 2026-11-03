//! Configuration resolution and connection blocking, for tests outside this
//! module.

use super::{ServerConfigError, ServerConfigurationHandle};
use xmtp_api::ApiClientWrapper;
use xmtp_db::prelude::DbQuery;
use xmtp_proto::api_client::XmtpBackendClient;

/// Resolve the configuration a client starts with from `db` and `api`, as
/// a build does.
pub async fn resolve<ApiClient>(
    api: &ApiClientWrapper<ApiClient>,
    db: &impl DbQuery,
    allow_offline: bool,
) -> Result<ServerConfigurationHandle, ServerConfigError>
where
    ApiClient: XmtpBackendClient,
{
    super::resolve(api, db, allow_offline).await
}

/// Block `handle`'s connection for `reason` and return the cause that
/// holds, as a refresh that meets a refusal does.
#[cfg(any(test, feature = "test-hooks"))]
pub fn block_connection(
    handle: &ServerConfigurationHandle,
    reason: super::BlockedConnection,
) -> super::BlockedConnection {
    handle.block_connection(reason)
}

#[cfg(not(target_arch = "wasm32"))]
pub mod scripted_api;

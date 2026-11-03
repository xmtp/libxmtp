//! The deployment a data directory recorded, for client tests outside this
//! module.

use super::{StorageLocation, StorageLocationError};

/// The deployment identifier `location` recorded for `backend_url`.
///
/// # Panics
///
/// When `location` is not a data directory, which records no deployment.
pub async fn recorded_deployment(
    location: &StorageLocation,
    backend_url: &str,
) -> Result<Option<String>, StorageLocationError> {
    location
        .recorder(backend_url)
        .expect("a data directory records its deployment")
        .lookup()
        .await
}

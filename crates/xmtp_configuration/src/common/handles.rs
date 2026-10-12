//! Handle configuration values

/// Default minimum number of characters in a handle's name part. The backend
/// applies it when no `handles.min_name_length` is configured, and a client
/// uses it when a snapshot does not carry the value.
pub const BACKEND_DEFAULT_HANDLE_MIN_NAME_LENGTH: u32 = 3;

//! Signing for an attachment storage target. The settings are
//! `xmtp_backend_config::attachments`, validated before a target is built.

mod s3;

pub use s3::{BuildError, PresignedRequest, S3Target, SignError, StorageTarget, build_target};

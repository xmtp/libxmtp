//! Configuration and signing for an attachment storage target.

mod config;
mod s3;

pub use config::{AttachmentsConfig, ConfigInvalid, CredentialsConfig, S3Config, TargetConfig};
pub use s3::{BuildError, PresignedRequest, S3Target, SignError, StorageTarget, build_target};

//! A configuration that passed validation, and views of its sections.

use std::{num::NonZeroUsize, ops::Deref, path::Path};

use crate::{
    Config, ConfigError,
    attachments::{AttachmentsConfig, S3Config, TargetConfig},
};

/// A configuration that passed every document check. Only `Config::validate`
/// builds one, so holding one proves the checks ran. Its sections are read
/// through `Deref` and cannot be changed: change the file layer from
/// `into_config` and validate again.
#[derive(Clone, Debug)]
pub struct ValidatedConfig {
    config: Config,
    scw_cache_entries: NonZeroUsize,
}

impl ValidatedConfig {
    pub(crate) fn new(config: Config, scw_cache_entries: NonZeroUsize) -> Self {
        Self {
            config,
            scw_cache_entries,
        }
    }

    /// Read one TOML file, resolve its environment references, and validate it.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Config::from_path(path)?.validate()
    }

    /// Parse one TOML document, resolve its environment references, and
    /// validate it. Errors omit document contents. This method cannot return
    /// `ConfigError::Read`.
    pub fn load_str(contents: &str) -> Result<Self, ConfigError> {
        Config::from_toml_str(contents)?.validate()
    }

    /// The identifier every client binds to. Validation requires one.
    pub fn identifier(&self) -> &str {
        self.config.server.identifier.as_deref().unwrap_or_default()
    }

    /// Whether this deployment checks credentials. Validation requires an
    /// `[auth]` table to say so.
    pub fn auth_enabled(&self) -> bool {
        self.config
            .auth
            .as_ref()
            .is_some_and(|auth| auth.enabled == Some(true))
    }

    /// The capacity of the smart contract wallet signature cache.
    pub fn scw_cache_entries(&self) -> NonZeroUsize {
        self.scw_cache_entries
    }

    /// The attachment settings, when this deployment offers attachments.
    pub fn attachments(&self) -> Option<Validated<'_, AttachmentsConfig>> {
        self.config.attachments.as_ref().map(Validated)
    }

    /// The file layer, for a change followed by validation again.
    pub fn into_config(self) -> Config {
        self.config
    }
}

impl Deref for ValidatedConfig {
    type Target = Config;

    fn deref(&self) -> &Config {
        &self.config
    }
}

/// A section of a `ValidatedConfig`. Only a `ValidatedConfig` hands one out,
/// so a function that takes one gets settings that passed validation.
#[derive(Debug)]
pub struct Validated<'a, T>(&'a T);

impl<T> Clone for Validated<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Validated<'_, T> {}

impl<T> Deref for Validated<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a> Validated<'a, AttachmentsConfig> {
    /// The S3-compatible storage target's settings.
    pub fn s3(self) -> Validated<'a, S3Config> {
        match &self.0.target {
            TargetConfig::S3(s3) => Validated(s3),
        }
    }
}

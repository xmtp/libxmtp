//! Compile only in unit‑tests or with the `test‑utils` feature.
#![cfg(any(test, feature = "test-utils"))]

use crate::groups::mls_sync::GroupMessageProcessingError;
use crate::groups::mls_sync::GroupMessageProcessingError::OpenMlsProcessMessage;
use openmls::group::ProcessMessageError;
use openmls::group::ValidationError::WrongEpoch;
use openmls::prelude::Lifetime;
use std::{env, fmt};

#[derive(Copy, Clone)]
struct EnvFlag(&'static str);

impl EnvFlag {
    #[inline]
    fn set(self, enable: bool) {
        env::set_var(self.0, enable.to_string());
    }

    #[inline]
    fn get(self) -> bool {
        env::var(self.0).ok().as_deref() == Some("true")
    }

    #[allow(dead_code)]
    #[inline]
    fn clear(self) {
        env::remove_var(self.0);
    }
}

impl fmt::Debug for EnvFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EnvFlag").field(&self.0).finish()
    }
}

const UPLOAD_MALFORMED_KP: EnvFlag = EnvFlag("TEST_MODE_UPLOAD_MALFORMED_KP");
const MALFORMED_INSTALLATIONS_KEY: &str = "TEST_MODE_MALFORMED_INSTALLATIONS";

const FUTURE_WRONG_EPOCH: EnvFlag = EnvFlag("TEST_MODE_FUTURE_WRONG_EPOCH");

const LIMIT_KP_LIFETIME: EnvFlag = EnvFlag("TEST_MODE_LIMIT_KP_LIFETIME");
const LIMIT_KP_LIFETIME_VALUE: &str = "TEST_MODE_LIMIT_KP_LIFETIME_VALUE";

/* ---------- Malformed KeyPackages ---------- */

/// Enable / disable *and* optionally inject a list of installations.
pub fn set_test_mode_upload_malformed_keypackage(
    enable: bool,
    installations: Option<Vec<Vec<u8>>>,
) {
    if enable {
        UPLOAD_MALFORMED_KP.set(true);
        // always reset the value key first to avoid leaking previous data
        env::remove_var(MALFORMED_INSTALLATIONS_KEY);

        if let Some(list) = installations {
            let joined = list.iter().map(hex::encode).collect::<Vec<_>>().join(",");
            env::set_var(MALFORMED_INSTALLATIONS_KEY, joined);
        }
    } else {
        UPLOAD_MALFORMED_KP.set(false);
        env::remove_var(MALFORMED_INSTALLATIONS_KEY);
    }
}

#[inline]
pub fn is_test_mode_upload_malformed_keypackage() -> bool {
    UPLOAD_MALFORMED_KP.get()
}

pub fn get_test_mode_malformed_installations() -> Vec<Vec<u8>> {
    if !is_test_mode_upload_malformed_keypackage() {
        return Vec::new();
    }

    env::var(MALFORMED_INSTALLATIONS_KEY)
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|hex| hex::decode(hex).ok())
        .collect()
}

/* ---------- wrong / future epoch helpers ---------- */

/// Toggle wrong‑epoch test‑mode.
#[inline]
pub fn set_test_mode_future_wrong_epoch(enable: bool) {
    FUTURE_WRONG_EPOCH.set(enable);
}

#[inline]
pub fn is_test_mode_future_wrong_epoch() -> bool {
    FUTURE_WRONG_EPOCH.get()
}

/// If the flag is set, fail with a *wrong epoch* validation error.
pub fn maybe_mock_wrong_epoch_for_tests() -> Result<(), GroupMessageProcessingError> {
    if is_test_mode_future_wrong_epoch() {
        return Err(OpenMlsProcessMessage(ProcessMessageError::ValidationError(
            WrongEpoch,
        )));
    }
    Ok(())
}

/// If the flag is set, fail with a *future epoch* error.
pub fn maybe_mock_future_epoch_for_tests() -> Result<(), GroupMessageProcessingError> {
    if is_test_mode_future_wrong_epoch() {
        return Err(GroupMessageProcessingError::FutureEpoch(10, 0));
    }
    Ok(())
}

/* ---------- KeyPackage lifetime limiter ---------- */

const DEFAULT_KP_LIFETIME: u64 = 2_592_000; // 30 days

pub fn set_test_mode_limit_key_package_lifetime(enable: bool, seconds: u64) {
    if enable {
        LIMIT_KP_LIFETIME.set(true);
        env::set_var(LIMIT_KP_LIFETIME_VALUE, seconds.to_string());
    } else {
        LIMIT_KP_LIFETIME.set(false);
        env::remove_var(LIMIT_KP_LIFETIME_VALUE);
    }
}

#[inline]
pub fn is_test_mode_limit_key_package_lifetime() -> bool {
    LIMIT_KP_LIFETIME.get()
}

pub fn get_test_mode_limit_key_package_lifetime() -> u64 {
    if !is_test_mode_limit_key_package_lifetime() {
        return DEFAULT_KP_LIFETIME;
    }
    env::var(LIMIT_KP_LIFETIME_VALUE)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_KP_LIFETIME)
}

/// If the flag is set, will return mocked lifetime
pub fn maybe_mock_package_lifetime() -> Lifetime {
    if is_test_mode_limit_key_package_lifetime() {
        Lifetime::new(get_test_mode_limit_key_package_lifetime())
    } else {
        Lifetime::default()
    }
}

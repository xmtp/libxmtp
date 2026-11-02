//! Key-package faults that tests inject into fetching and building packages.

use openmls::prelude::Lifetime;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fmt};
use xmtp_id::key_package::VerifiedKeyPackageV2;

#[derive(Copy, Clone)]
struct EnvFlag(&'static str);

impl EnvFlag {
    #[inline]
    fn set(self, enable: bool) {
        unsafe { env::set_var(self.0, enable.to_string()) };
    }

    #[inline]
    fn get(self) -> bool {
        env::var(self.0).ok().as_deref() == Some("true")
    }

    #[allow(dead_code)]
    #[inline]
    fn clear(self) {
        unsafe { env::remove_var(self.0) };
    }
}

impl fmt::Debug for EnvFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("EnvFlag").field(&self.0).finish()
    }
}

static MALFORMED_INSTALLATIONS: Mutex<Option<Vec<Vec<u8>>>> = Mutex::new(None);

static UNCHECKED_KEY_PACKAGES: Mutex<Vec<VerifiedKeyPackageV2>> = Mutex::new(Vec::new());

static INJECTED_KEY_PACKAGES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

static MOCKED_LIFETIMES: AtomicUsize = AtomicUsize::new(0);

const LIMIT_KP_LIFETIME: EnvFlag = EnvFlag("TEST_MODE_LIMIT_KP_LIFETIME");
const LIMIT_KP_LIFETIME_VALUE: &str = "TEST_MODE_LIMIT_KP_LIFETIME_VALUE";

/* ---------- Malformed KeyPackages ---------- */

/// Enable / disable *and* optionally inject a list of installations.
pub fn set_test_mode_upload_malformed_keypackage(
    enable: bool,
    installations: Option<Vec<Vec<u8>>>,
) {
    *MALFORMED_INSTALLATIONS.lock() = enable.then(|| installations.unwrap_or_default());
}

/// Resets the malformed-package list when dropped, even on panic.
pub struct RestoreKeyPackages;
impl Drop for RestoreKeyPackages {
    fn drop(&mut self) {
        set_test_mode_upload_malformed_keypackage(false, None);
    }
}

#[inline]
pub fn is_test_mode_upload_malformed_keypackage() -> bool {
    MALFORMED_INSTALLATIONS.lock().is_some()
}

pub fn get_test_mode_malformed_installations() -> Vec<Vec<u8>> {
    MALFORMED_INSTALLATIONS.lock().clone().unwrap_or_default()
}

/* ---------- KeyPackages that skip the sender's checks ---------- */

/// A sender uses `package` for its installation in place of the package the
/// backend serves, and adds it without verifying it, as a client from before
/// a verification rule would. Build `package` by hand to skip the checks.
pub fn set_test_mode_unchecked_key_package(package: VerifiedKeyPackageV2) {
    UNCHECKED_KEY_PACKAGES.lock().push(package);
}

/// The unchecked package configured for `installation_id`, if any.
pub fn test_mode_unchecked_key_package(installation_id: &[u8]) -> Option<VerifiedKeyPackageV2> {
    UNCHECKED_KEY_PACKAGES
        .lock()
        .iter()
        .find(|package| package.installation_id() == installation_id)
        .cloned()
}

/* ---------- Faults that ran ---------- */

/// Record that a fetch dropped or replaced the package of `installation_id`.
pub fn record_injected_key_package(installation_id: &[u8]) {
    INJECTED_KEY_PACKAGES.lock().push(installation_id.to_vec());
}

/// Whether a fetch dropped or replaced the package of `installation_id`, so a
/// test can tell that its fault ran.
#[cfg(test)]
pub fn key_package_injected(installation_id: &[u8]) -> bool {
    INJECTED_KEY_PACKAGES
        .lock()
        .iter()
        .any(|id| id == installation_id)
}

/// How many packages were built with the limited lifetime.
#[cfg(test)]
pub fn mocked_package_lifetimes() -> usize {
    MOCKED_LIFETIMES.load(Ordering::SeqCst)
}

/* ---------- KeyPackage lifetime limiter ---------- */

const DEFAULT_KP_LIFETIME: u64 = 2_592_000; // 30 days

pub fn set_test_mode_limit_key_package_lifetime(enable: bool, seconds: u64) {
    if enable {
        LIMIT_KP_LIFETIME.set(true);
        unsafe { env::set_var(LIMIT_KP_LIFETIME_VALUE, seconds.to_string()) };
    } else {
        LIMIT_KP_LIFETIME.set(false);
        unsafe { env::remove_var(LIMIT_KP_LIFETIME_VALUE) };
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
        MOCKED_LIFETIMES.fetch_add(1, Ordering::SeqCst);
        Lifetime::new(get_test_mode_limit_key_package_lifetime())
    } else {
        Lifetime::default()
    }
}

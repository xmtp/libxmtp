pub mod basic_credential;
pub mod configuration;
pub mod ethereum;
pub mod hash;
pub mod rand;
pub mod signature;
pub mod utils;

pub use basic_credential::*;
pub use openmls;

pub type Secret = tls_codec::SecretVLBytes; // Byte array with ZeroizeOnDrop

// When upgrading to reqwest 0.13 and disabling the default aws-lc-rs crypto provider
// some tests fail without initializing the ring crypto provider. Doing this here because
// it is crypto related and all crates depend on this crate.
//
// Installs the process-default rustls crypto provider. Idempotent: subsequent calls are
// cheap no-ops via `Once`, and `install_default` returning `Err` (provider already set) is
// ignored, so it is safe to call even when a host process pre-installs its own provider.
//
// This is exposed publicly and must be called explicitly from binding entry points because
// the `#[ctor::ctor(unsafe)]` below does not run on Apple platforms: `ctor` relies on the
// `__DATA,__mod_init_func` Mach-O link section, which Apple no longer supports. When
// `libxmtpv3.a` is statically linked into a Swift binary the constructor record is orphaned
// and never fires, leaving the provider uninstalled and causing `reqwest` (rustls-no-provider)
// to `panic!("No provider set")` on the first HTTP client build. See issue #3846.
#[cfg(not(target_arch = "wasm32"))]
pub fn install_crypto_provider() {
    use std::sync::Once;
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[ctor::ctor(unsafe)]
fn install_rustls_crypto_provider() {
    install_crypto_provider();
}

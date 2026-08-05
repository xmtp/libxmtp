//! Construction of the HTTP clients libxmtp uses for plain HTTP(S) endpoints.
//!
//! Today that is the history server: device-sync archive upload and download.
//! gRPC traffic does not go through here — it is configured in `xmtp_api_grpc`.

/// Build a [`reqwest::Client`] for talking to XMTP HTTP endpoints.
///
/// Prefer this over `reqwest::Client::new()` anywhere in the workspace: on Android a
/// default-built client aborts the process the first time it opens a TLS connection.
/// reqwest 0.13 reaches for `rustls-platform-verifier` whenever no explicit roots are
/// configured, and that verifier calls into the JVM on Android. It panics with
/// `Expect rustls-platform-verifier to be initialized` unless the *host application*
/// initializes it over JNI with a `Context` and also ships the crate's Kotlin component.
/// libxmtp is consumed as a plain `.so` through uniffi, so that initialization never
/// happens, and the device-sync worker took the whole app down on its first archive
/// transfer.
///
/// So on Android we hand reqwest a rustls config built from the bundled webpki roots and
/// the platform verifier is never constructed. This mirrors `xmtp_api_grpc`, which
/// already forces webpki roots for the gRPC channel on Android and iOS.
///
/// Other platforms keep reqwest's default behaviour: the platform verifier works there
/// without any initialization, and it honours user- and enterprise-installed CAs.
pub fn client() -> Result<reqwest::Client, reqwest::Error> {
    client_builder().build()
}

/// The same configuration as [`client`], for callers that need to set timeouts or other
/// options before building. Note that on Android the TLS setup is already fixed here, so
/// reqwest's own TLS options (extra roots, `danger_accept_invalid_certs`, ...) are ignored.
// The one place in the workspace allowed to construct a reqwest client directly; `.clippy.toml`
// disallows it everywhere else so the Android TLS setup below cannot be bypassed.
#[allow(clippy::disallowed_methods)]
pub fn client_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    #[cfg(target_os = "android")]
    let builder = builder.use_preconfigured_tls(bundled_roots_tls_config());
    builder
}

/// A rustls config that trusts the webpki (Mozilla) root store compiled into the binary,
/// with no dependency on the platform trust store or on any runtime initialization.
///
/// Compiled outside Android as well so the test below can exercise it in CI.
#[cfg(all(not(target_family = "wasm"), any(target_os = "android", test)))]
fn bundled_roots_tls_config() -> rustls::ClientConfig {
    // `ClientConfig::builder` resolves the process-default crypto provider. Installing it
    // first means that lookup always finds one instead of falling back to rustls'
    // crate-feature path, which panics when zero or several provider features are enabled.
    xmtp_cryptography::install_crypto_provider();

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    // reqwest applies its ALPN preferences only to configs it builds itself; a
    // preconfigured config is used verbatim. The workspace builds reqwest without its
    // `http2` feature, so http/1.1 is the only protocol the connector can speak — offering
    // `h2` here would let a server negotiate a protocol the client cannot follow.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;

    /// `use_preconfigured_tls` takes an `Any`: when the rustls version we build the config
    /// with does not match the one reqwest was compiled against, reqwest silently records an
    /// "unknown" TLS backend and only errors at `build()`. That would be an Android-only
    /// failure, so pin it down here instead.
    #[xmtp_common::test(unwrap_try = true)]
    #[allow(clippy::disallowed_methods)]
    fn bundled_roots_config_is_accepted_by_reqwest() {
        let config = bundled_roots_tls_config();
        assert!(!config.alpn_protocols.is_empty());

        reqwest::Client::builder()
            .use_preconfigured_tls(config)
            .build()
            .expect("reqwest did not recognize our preconfigured rustls config");
    }
}

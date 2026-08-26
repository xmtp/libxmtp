//! Sentry backend for the telemetry slot: client + SentryLayer construction,
//! FFI-boundary event promotion, and HKDF pseudonymization of identity values.

use std::sync::{Arc, RwLock};

use crate::error::Error;

const HKDF_SALT: &[u8] = b"convos-metrics";

/// Normalized identity field keys and the HKDF `info` class each hashes under.
/// Values of these keys are pseudonymized before leaving the device for Sentry;
/// local layers (logcat/oslog/file) keep raw values.
pub(crate) const ID_KEYS: &[(&str, &[u8])] = &[
    ("inbox_id", b"inbox-stable-id"),
    ("sender_inbox_id", b"inbox-stable-id"),
    ("proposer_inbox_id", b"inbox-stable-id"),
    ("removed_inbox_id", b"inbox-stable-id"),
    ("claimed_inbox_id", b"inbox-stable-id"),
    ("group_id", b"group-stable-id"),
    ("dm_id", b"group-stable-id"),
    ("installation_id", b"installation-stable-id"),
    ("sender_installation_id", b"installation-stable-id"),
    ("actor_installation_id", b"installation-stable-id"),
    ("message_id", b"message-stable-id"),
    ("topic", b"topic-stable-id"),
    ("entity_id", b"entity-stable-id"),
];

pub(crate) fn stable_id(value: &str, info: &[u8]) -> String {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), value.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("32 bytes is a valid HKDF-SHA256 length");
    hex::encode(okm)
}

/// `sentry_tracing` renames span-inherited fields to `"{span_name}:{key}"`, and
/// strips the `tags.` prefix on the event path but *not* the breadcrumb one, so
/// normalize both before matching (`"sync_group:tags.dm_id"` -> `"dm_id"`).
pub(crate) fn scrub_value(key: &str, value: &str) -> Option<String> {
    let key = key.rsplit(':').next().unwrap_or(key);
    let key = key.strip_prefix("tags.").unwrap_or(key);
    ID_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, info)| stable_id(value, info))
}

fn scrub_map(map: &mut sentry::protocol::Map<String, sentry::protocol::Value>) {
    for (key, value) in map.iter_mut() {
        if let sentry::protocol::Value::String(s) = value
            && let Some(hashed) = scrub_value(key, s)
        {
            *s = hashed;
        }
    }
}

/// Sentry backend configuration. `user_stable_id` is the HKDF stable id
/// computed by the host app (matches the Convos iOS PostHog person id).
#[derive(Debug, Clone)]
pub struct SentryConfig {
    pub dsn: String,
    pub environment: Option<String>,
    pub release: Option<String>,
    pub traces_sample_rate: f32,
    pub max_breadcrumbs: usize,
    pub user_stable_id: Option<String>,
    pub tags: Vec<(String, String)>,
}

impl Default for SentryConfig {
    fn default() -> Self {
        Self {
            dsn: String::new(),
            environment: None,
            release: None,
            traces_sample_rate: 0.0,
            max_breadcrumbs: 100,
            user_stable_id: None,
            tags: Vec::new(),
        }
    }
}

static USER_STABLE_ID: RwLock<Option<String>> = RwLock::new(None);

/// Set (or clear) the pseudonymous user id stamped onto outgoing error events.
/// A global slot read at capture time in `before_send`, so late identify (at
/// inbox-ready) applies to already-bound worker hubs too.
pub fn set_user_stable_id(id: Option<String>) {
    *USER_STABLE_ID.write().unwrap_or_else(|e| e.into_inner()) = id;
}

/// ERROR events from the mobile FFI crate become Sentry issues; every other
/// ERROR/WARN/INFO event is context (breadcrumb). Interior `instrument(err)`
/// frames therefore never file duplicate issues for one propagating error.
pub(crate) fn event_filter(meta: &tracing::Metadata<'_>) -> sentry_tracing::EventFilter {
    use sentry_tracing::EventFilter;
    use tracing::Level;
    match *meta.level() {
        Level::ERROR if meta.target().starts_with("xmtpv3") => EventFilter::Event,
        Level::ERROR | Level::WARN | Level::INFO => EventFilter::Breadcrumb,
        _ => EventFilter::Ignore,
    }
}

/// Spans carrying `sentry.op` (the span macros, incl. trace-level `err_span`
/// FFI roots) always pass; other spans pass at DEBUG or more severe.
pub(crate) fn span_filter(meta: &tracing::Metadata<'_>) -> bool {
    meta.fields().field("sentry.op").is_some() || *meta.level() <= tracing::Level::DEBUG
}

fn scrub_breadcrumb(mut b: sentry::Breadcrumb) -> Option<sentry::Breadcrumb> {
    scrub_map(&mut b.data);
    Some(b)
}

/// `sentry_tracing` writes event fields to `contexts["Rust Tracing Fields"]` and
/// `tags.`-prefixed fields to `tags`; the scope's span copies its data into
/// `contexts["trace"]`. `extra` only ever holds scope-set values.
fn scrub_event(
    mut e: sentry::protocol::Event<'static>,
) -> Option<sentry::protocol::Event<'static>> {
    for context in e.contexts.values_mut() {
        match context {
            sentry::protocol::Context::Other(map) => scrub_map(map),
            sentry::protocol::Context::Trace(trace) => scrub_map(&mut trace.data),
            _ => {}
        }
    }
    for (key, value) in e.tags.iter_mut() {
        if let Some(hashed) = scrub_value(key, value) {
            *value = hashed;
        }
    }
    scrub_map(&mut e.extra);
    if let Some(id) = USER_STABLE_ID
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        e.user = Some(sentry::User {
            id: Some(id),
            ..Default::default()
        });
    }
    Some(e)
}

/// The workspace pins sentry/reqwest to `rustls-no-provider`, so the transport
/// built inside `sentry::init` panics unless a process-default provider exists.
/// Idempotent, and a host- or `xmtp_cryptography`-installed provider wins.
fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

/// Client options minus the DSN: sampling, breadcrumb cap, and the scrub hooks.
/// Shared with the tests, which supply their own capturing transport + DSN.
fn client_options(cfg: &SentryConfig) -> Result<sentry::ClientOptions, Error> {
    if !(0.0..=1.0).contains(&cfg.traces_sample_rate) {
        return Err(Error::Telemetry(format!(
            "traces_sample_rate {} outside [0.0, 1.0]",
            cfg.traces_sample_rate
        )));
    }
    let mut options = sentry::ClientOptions::new()
        .sample_rate(1.0)
        .traces_sample_rate(cfg.traces_sample_rate);
    options.release = cfg
        .release
        .clone()
        .map(Into::into)
        .or_else(|| sentry::release_name!());
    options.environment = cfg.environment.clone().map(Into::into);
    options.max_breadcrumbs = cfg.max_breadcrumbs;
    options.send_default_pii = false;
    options.before_send = Some(Arc::new(scrub_event));
    options.before_breadcrumb = Some(Arc::new(scrub_breadcrumb));
    Ok(options)
}

pub(crate) fn build_sentry_layer(
    cfg: SentryConfig,
) -> Result<(crate::handle::BoxLayer, sentry::ClientInitGuard), Error> {
    use tracing_subscriber::Layer;
    let dsn: sentry::types::Dsn = cfg
        .dsn
        .parse()
        .map_err(|_| Error::Telemetry(format!("invalid sentry dsn: {}", cfg.dsn)))?;
    let mut options = client_options(&cfg)?;
    options.dsn = Some(dsn);
    // Only write the slot when the caller has an id: a re-init with `None` must
    // not clobber an identify that already landed.
    if cfg.user_stable_id.is_some() {
        set_user_stable_id(cfg.user_stable_id.clone());
    }
    install_crypto_provider();
    let guard = sentry::init(options);
    sentry::configure_scope(|scope| {
        scope.set_tag("component", "libxmtp");
        for (k, v) in &cfg.tags {
            scope.set_tag(k, v);
        }
    });
    let layer = sentry_tracing::layer()
        .event_filter(event_filter)
        .span_filter(span_filter);
    Ok((layer.boxed(), guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors computed with reference HKDF-SHA256 (extract+expand), matching
    // Convos iOS MetricsStableIdEncoder(salt: "convos-metrics", info: <class>).
    #[test]
    fn stable_id_matches_reference_vectors() {
        assert_eq!(
            stable_id(
                "a9be0a2ae5aca34ff1a4bd25277f7e56e3b32e4b418ba1b48f0d33d004cd4b9a",
                b"inbox-stable-id"
            ),
            "68a7e54af6e50bf438d86104d6ea9e25ffa0cfe80ceac0a5497db459da371074"
        );
        assert_eq!(
            stable_id("d2794b1478b6e0a06d3bd1a52a3aae1c", b"group-stable-id"),
            "97a6abdc3b86714e001232db751893461d16038c4dc03a24c1e9e648e6485675"
        );
        assert_eq!(
            stable_id("8ab721a3", b"installation-stable-id"),
            "d0687fd03b5e3aa19778ba0852972ff8d9c8d6ae5f542c5b2acc1d6a2dc0ce61"
        );
        assert_eq!(
            stable_id("aa11", b"message-stable-id"),
            "9d453b36d1515cc0447304e9a6646ed7d5044803d60d1cc224e394e221baf24f"
        );
        assert_eq!(
            stable_id("/xmtp/mls1/g-aa11/proto", b"topic-stable-id"),
            "8485ce3ec2b1ba541c19b4fcce2f7c46874af297a8013f478cb0c021634b5a61"
        );
    }

    #[test]
    fn scrub_value_hashes_only_id_keys() {
        let hashed = scrub_value("inbox_id", "a9be").unwrap();
        assert_eq!(hashed.len(), 64);
        assert_ne!(hashed, "a9be");
        assert_eq!(scrub_value("sender_inbox_id", "a9be").unwrap(), hashed);
        assert!(scrub_value("cursor", "123").is_none());
        assert!(scrub_value("operation", "mls.sync").is_none());

        // entity_id hashes under its own class, not inbox_id's.
        let entity = scrub_value("entity_id", "a9be").unwrap();
        assert_eq!(entity.len(), 64);
        assert!(entity.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(entity, "a9be");
        assert_ne!(entity, hashed);

        // Span-qualified and `tags.`-prefixed keys normalize to the same id key.
        let dm = scrub_value("dm_id", "d279").unwrap();
        assert_eq!(scrub_value("tags.dm_id", "d279").unwrap(), dm);
        assert_eq!(scrub_value("sync_group:dm_id", "d279").unwrap(), dm);
        assert_eq!(scrub_value("sync_group:tags.dm_id", "d279").unwrap(), dm);
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;

    static USER_SLOT: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn user_stable_id() -> Option<String> {
        USER_STABLE_ID
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// `USER_STABLE_ID` is process-global: serialize the tests that write it and
    /// restore the prior value on drop, so a failed assertion can't leave the
    /// slot dirty for the next test.
    struct UserSlot {
        _lock: std::sync::MutexGuard<'static, ()>,
        previous: Option<String>,
    }

    impl Drop for UserSlot {
        fn drop(&mut self) {
            set_user_stable_id(self.previous.take());
        }
    }

    fn lock_user_slot() -> UserSlot {
        let _lock = USER_SLOT.lock().unwrap_or_else(|e| e.into_inner());
        UserSlot {
            _lock,
            previous: user_stable_id(),
        }
    }

    // Metadata can't be constructed directly; capture it via a subscriber.
    fn metadata_of(f: impl FnOnce()) -> &'static tracing::Metadata<'static> {
        use std::sync::Mutex;
        static CAPTURED: Mutex<Option<&'static tracing::Metadata<'static>>> = Mutex::new(None);
        struct Cap;
        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Cap {
            fn on_event(
                &self,
                event: &tracing::Event<'_>,
                _ctx: tracing_subscriber::layer::Context<'_, S>,
            ) {
                *CAPTURED.lock().unwrap() = Some(event.metadata());
            }
        }
        use tracing_subscriber::prelude::*;
        let subscriber = tracing_subscriber::registry().with(Cap);
        tracing::subscriber::with_default(subscriber, f);
        CAPTURED.lock().unwrap().take().expect("event fired")
    }

    fn span_metadata_of(f: impl FnOnce()) -> &'static tracing::Metadata<'static> {
        use std::sync::Mutex;
        static CAPTURED: Mutex<Option<&'static tracing::Metadata<'static>>> = Mutex::new(None);
        struct Cap;
        impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Cap {
            fn on_new_span(
                &self,
                attrs: &tracing::span::Attributes<'_>,
                _id: &tracing::span::Id,
                _ctx: tracing_subscriber::layer::Context<'_, S>,
            ) {
                *CAPTURED.lock().unwrap() = Some(attrs.metadata());
            }
        }
        use tracing_subscriber::prelude::*;
        let subscriber = tracing_subscriber::registry().with(Cap);
        tracing::subscriber::with_default(subscriber, f);
        CAPTURED.lock().unwrap().take().expect("span created")
    }

    #[test]
    fn spans_pass_on_sentry_op_or_severity() {
        let op = span_metadata_of(|| {
            let _s = tracing::trace_span!("ffi_root", sentry.op = "ffi");
        });
        let plain_trace = span_metadata_of(|| {
            let _s = tracing::trace_span!("noisy");
        });
        let debug = span_metadata_of(|| {
            let _s = tracing::debug_span!("detail");
        });
        let info = span_metadata_of(|| {
            let _s = tracing::info_span!("sync");
        });
        assert!(span_filter(op));
        assert!(!span_filter(plain_trace));
        assert!(span_filter(debug));
        assert!(span_filter(info));
    }

    #[test]
    fn error_events_promote_only_at_ffi_boundary() {
        use sentry_tracing::EventFilter;
        let ffi = metadata_of(|| tracing::error!(target: "xmtpv3::mls", "boom"));
        let inner = metadata_of(|| tracing::error!(target: "xmtp_mls::groups", "boom"));
        let info = metadata_of(|| tracing::info!(target: "xmtp_mls::groups", "hello"));
        let debug = metadata_of(|| tracing::debug!(target: "xmtp_mls::groups", "detail"));
        assert_eq!(event_filter(ffi).bits(), EventFilter::Event.bits());
        assert_eq!(event_filter(inner).bits(), EventFilter::Breadcrumb.bits());
        assert_eq!(event_filter(info).bits(), EventFilter::Breadcrumb.bits());
        assert_eq!(event_filter(debug).bits(), EventFilter::Ignore.bits());
    }

    #[test]
    fn breadcrumb_scrub_hashes_ids_and_user_is_stamped() {
        let _slot = lock_user_slot();
        set_user_stable_id(Some("stable-user".into()));
        let events = sentry::test::with_captured_events_options(
            || {
                sentry::add_breadcrumb(sentry::Breadcrumb {
                    data: [(
                        "group_id".to_string(),
                        sentry::protocol::Value::String("d2794b1478b6e0a06d3bd1a52a3aae1c".into()),
                    )]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                });
                sentry::capture_message("boom", sentry::Level::Error);
            },
            client_options(&SentryConfig::default()).unwrap(),
        );
        let event = &events[0];
        assert_eq!(
            event.user.as_ref().unwrap().id.as_deref(),
            Some("stable-user")
        );
        let crumb = &event.breadcrumbs.values[0];
        assert_eq!(
            crumb.data["group_id"],
            sentry::protocol::Value::String(
                "97a6abdc3b86714e001232db751893461d16038c4dc03a24c1e9e648e6485675".into()
            )
        );
    }

    #[test]
    fn crypto_provider_is_installed_for_the_transport() {
        install_crypto_provider();
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    /// The transport is `rustls-no-provider`: building the client panics without
    /// a process-default provider, so this is the only test that proves the real
    /// reqwest/rustls transport constructs (`EnvelopeSender::new` builds it
    /// eagerly inside `sentry::init`). `default_integrations(false)` keeps the
    /// real init from installing sentry's panic hook for the rest of the binary,
    /// and the tiny `shutdown_timeout` bounds the guard drop.
    #[test]
    fn client_builds_a_real_transport_without_a_host_installed_provider() {
        let mut options = client_options(&SentryConfig::default()).unwrap();
        options.dsn = Some("https://public@o0.ingest.sentry.io/0".parse().unwrap());
        options.default_integrations = false;
        options.shutdown_timeout = std::time::Duration::from_millis(1);
        install_crypto_provider();
        let guard = sentry::init(options);
        assert!(guard.is_enabled());
        drop(guard);
    }

    #[test]
    fn invalid_dsn_is_an_error() {
        let _slot = lock_user_slot();
        set_user_stable_id(Some("sentinel".into()));
        let cfg = SentryConfig {
            dsn: "not-a-dsn".into(),
            user_stable_id: Some("clobbered".into()),
            ..Default::default()
        };
        assert!(matches!(build_sentry_layer(cfg), Err(Error::Telemetry(_))));
        assert_eq!(
            user_stable_id().as_deref(),
            Some("sentinel"),
            "a failed init must not touch the user slot"
        );
    }

    const INBOX_ID_RAW: &str = "a9be0a2ae5aca34ff1a4bd25277f7e56e3b32e4b418ba1b48f0d33d004cd4b9a";
    const INBOX_ID_HASH: &str = "68a7e54af6e50bf438d86104d6ea9e25ffa0cfe80ceac0a5497db459da371074";
    const GROUP_ID_RAW: &str = "d2794b1478b6e0a06d3bd1a52a3aae1c";
    const GROUP_ID_HASH: &str = "97a6abdc3b86714e001232db751893461d16038c4dc03a24c1e9e648e6485675";

    /// TRACE span: only `sentry.op` gets it past `span_filter`. Its `group_id`
    /// reaches the event renamed to `"sync_group:group_id"`, and also verbatim
    /// in the trace context copied off the transaction.
    #[tracing::instrument(
        level = "trace",
        name = "sync_group",
        skip_all,
        fields(group_id = %GROUP_ID_RAW, sentry.op = "mls")
    )]
    fn emit_error_in_span() {
        tracing::error!(
            target: "xmtpv3::test",
            inbox_id = %INBOX_ID_RAW,
            tags.dm_id = %GROUP_ID_RAW,
            "boom"
        );
    }

    /// End-to-end through the real `sentry_tracing` layer: identity values must
    /// be hashed in every container the converter writes (`contexts`, `tags`),
    /// under both plain and span-qualified keys.
    #[test]
    fn sentry_tracing_event_is_scrubbed_in_every_container() {
        use tracing_subscriber::prelude::*;
        let _slot = lock_user_slot();
        // Production default: the trace context is populated even unsampled.
        let options = client_options(&SentryConfig::default()).unwrap();
        let events = sentry::test::with_captured_events_options(
            || {
                let subscriber = tracing_subscriber::registry().with(
                    sentry_tracing::layer()
                        .event_filter(event_filter)
                        .span_filter(span_filter)
                        .enable_span_attributes(),
                );
                tracing::subscriber::with_default(subscriber, || {
                    // Non-FFI WARN -> breadcrumb. `breadcrumb_from_event` does not
                    // strip the `tags.` prefix the event path removes, so the key
                    // arrives verbatim and still has to be normalized.
                    tracing::warn!(
                        target: "xmtp_mls::test",
                        group_id = %GROUP_ID_RAW,
                        tags.dm_id = %GROUP_ID_RAW,
                        "ctx"
                    );
                    emit_error_in_span();
                });
            },
            options,
        );
        assert_eq!(events.len(), 1);
        let event = &events[0];

        let crumb = event
            .breadcrumbs
            .values
            .iter()
            .find(|b| b.category.as_deref() == Some("xmtp_mls::test"))
            .expect("context breadcrumb");
        assert_eq!(
            crumb.data["tags.dm_id"],
            sentry::protocol::Value::String(GROUP_ID_HASH.into())
        );
        assert_eq!(
            crumb.data["group_id"],
            sentry::protocol::Value::String(GROUP_ID_HASH.into())
        );

        let fields = match event.contexts.get("Rust Tracing Fields") {
            Some(sentry::protocol::Context::Other(map)) => map,
            other => panic!("expected a tracing-fields context, got {other:?}"),
        };
        assert_eq!(
            fields["sync_group:group_id"],
            sentry::protocol::Value::String(GROUP_ID_HASH.into())
        );
        assert_eq!(
            fields["inbox_id"],
            sentry::protocol::Value::String(INBOX_ID_HASH.into())
        );
        assert_eq!(event.tags["dm_id"], GROUP_ID_HASH);

        // The scope copies the transaction's data verbatim into `contexts["trace"]`.
        let trace = match event.contexts.get("trace") {
            Some(sentry::protocol::Context::Trace(trace)) => trace,
            other => panic!("expected a trace context, got {other:?}"),
        };
        assert_eq!(
            trace.data["group_id"],
            sentry::protocol::Value::String(GROUP_ID_HASH.into())
        );

        let json = sentry::protocol::value::to_value(event)
            .unwrap()
            .to_string();
        assert!(!json.contains(GROUP_ID_RAW), "raw group_id in {json}");
        assert!(!json.contains(INBOX_ID_RAW), "raw inbox_id in {json}");
    }
}

use clap::{ArgGroup, Parser};
use sqlx::postgres::{PgConnectOptions, PgSslMode};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};
use xmtp_backend::{
    config::{Config, ConfigError, IDENTIFIER_ATTRIBUTE, LogFormat, LogLevel, ValidatedConfig},
    server, telemetry,
};

#[derive(Parser)]
#[command(group(ArgGroup::new("configuration").required(true).args(["config", "config_file"])))]
struct Args {
    /// Inline TOML document.
    #[arg(
        long,
        env = "XMTP_CONFIG",
        hide_env_values = true,
        conflicts_with = "config_file"
    )]
    config: Option<String>,
    /// Path to a TOML configuration file.
    #[arg(long, conflicts_with = "config")]
    config_file: Option<PathBuf>,
    /// Override server.log_level from the configuration.
    #[arg(long, value_enum)]
    log_level: Option<LogLevel>,
}

impl Args {
    /// Load the configuration, apply the command-line overrides to the file
    /// layer, and validate the result.
    fn config(&self) -> Result<ValidatedConfig, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = self.load_config()?;
        if let Some(level) = self.log_level {
            config.server.log_level = level;
        }
        Ok(config.validate()?)
    }

    /// The file layer of the configuration source, before any check.
    fn load_config(&self) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
        match &self.config {
            Some(contents) => {
                if std::fs::symlink_metadata(contents).is_ok() {
                    return Err(
                        "--config contains a file path; use --config-file for a TOML file".into(),
                    );
                }
                Ok(Config::from_toml_str(contents)?)
            }
            None => {
                Ok(Config::from_path(self.config_file.as_ref().expect(
                    "clap requires --config-file when --config is absent",
                ))?)
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    xmtp_cryptography::install_crypto_provider();
    let config = Args::parse().config()?;
    run(config, shutdown()).await
}

/// Install telemetry before database startup, then flush export after the transport drain.
async fn run(
    config: ValidatedConfig,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let otlp = otlp_export(&config)?;
    telemetry::install(&config.telemetry.metrics_listen)?;
    let stdout_level = logging_level(config.server.log_level);
    let level = match stdout_level {
        xmtp_logging::Level::Debug | xmtp_logging::Level::Trace => stdout_level,
        _ => xmtp_logging::Level::Info,
    };
    let logging = xmtp_logging::XmtpLogging::builder()
        .level(level)
        .stdout_level(stdout_level)
        .json(config.server.log_format == LogFormat::Json)
        .with_telemetry(otlp);
    let logging = logging.install()?;
    xmtp_logging::propagation::install();
    telemetry::describe();
    telemetry::ready(false);
    telemetry::info(env!("CARGO_PKG_VERSION"));
    for (key, reason) in unsafe_settings(&config) {
        tracing::warn!(key, "{reason}");
    }
    let address = config.server.listen.clone();
    let backend = server::initialize(config).await?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(listen = %listener.local_addr()?, "backend ready to serve");
    let result: Result<(), server::ServeError> = server::serve(backend, listener, shutdown).await;
    logging.disable_telemetry()?;
    result?;
    Ok(())
}

/// The shared logging level of a configured one.
fn logging_level(level: LogLevel) -> xmtp_logging::Level {
    match level {
        LogLevel::Off => xmtp_logging::Level::Off,
        LogLevel::Error => xmtp_logging::Level::Error,
        LogLevel::Warn => xmtp_logging::Level::Warn,
        LogLevel::Info => xmtp_logging::Level::Info,
        LogLevel::Debug => xmtp_logging::Level::Debug,
        LogLevel::Trace => xmtp_logging::Level::Trace,
    }
}

/// The OTLP export, if any. The endpoint is resolved before any exporter is
/// built, and every exported signal names this deployment.
fn otlp_export(
    config: &ValidatedConfig,
) -> Result<Option<xmtp_logging::TelemetryConfig>, ConfigError> {
    let telemetry = &config.telemetry;
    Ok(telemetry.otlp_endpoint_from_env()?.map(|endpoint| {
        let mut resource_attributes = telemetry.resource_attributes.clone();
        resource_attributes.insert(
            IDENTIFIER_ATTRIBUTE.to_owned(),
            config.identifier().to_owned(),
        );
        xmtp_logging::TelemetryConfig {
            endpoint: Some(endpoint),
            service_name: Some(telemetry.service_name.clone()),
            sample_ratio: telemetry.sample_ratio,
            logs: telemetry.otlp_logs,
            resource_attributes: resource_attributes.into_iter().collect(),
        }
    }))
}

/// Settings that admit any caller, or leave the store or a chain link
/// unverified, beyond this host. Names keys and modes, never values.
// implements: CONF-095
fn unsafe_settings(config: &ValidatedConfig) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let local = config
        .server
        .listen
        .parse::<SocketAddr>()
        .is_ok_and(|address| address.ip().is_loopback());
    if !config.auth_enabled() && !local {
        found.push((
            "auth".into(),
            format!(
                "disabled; {} serves every caller without a credential",
                config.server.listen
            ),
        ));
    }
    let stores = [
        ("database.url", Some(&config.database.url)),
        ("database.replica_url", config.database.replica_url.as_ref()),
    ];
    for (key, url) in stores {
        // sqlx's own parser, so PGSSLMODE and PGHOST fold in as at connect.
        let Some(options) = url.and_then(|url| PgConnectOptions::from_str(url).ok()) else {
            continue;
        };
        let mode = options.get_ssl_mode();
        // sqlx connects over a Unix socket when the host is a directory.
        let socket = options.get_socket().is_some() || options.get_host().starts_with('/');
        if !socket
            && !is_loopback_host(options.get_host())
            && !matches!(mode, PgSslMode::VerifyFull)
        {
            found.push((
                key.into(),
                format!("sslmode {mode:?} does not verify the server certificate and host"),
            ));
        }
    }
    for (chain, url) in &config.chains {
        let Ok(url) = url::Url::parse(url) else {
            continue;
        };
        if url.scheme() == "http" && !url.host_str().is_some_and(is_loopback_host) {
            found.push((
                format!("chains.\"{chain}\""),
                "plain HTTP to a host that is not loopback".into(),
            ));
        }
    }
    found
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

async fn shutdown() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("termination signal handler installs");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
    tracing::info!("backend shutdown requested");
}

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;

//! keepalive-probe — throwaway diagnostic for herald-lite #70.
//!
//! Opens `--count` idle HTTP/2 connections to an XMTP gRPC endpoint with a
//! configurable keepalive, holds each one doing nothing, and reports how long
//! each survives. Defaults mirror libxmtp's `apply_channel_options`
//! (`crates/xmtp_api_grpc/src/grpc_client/native.rs`), so a bare run reproduces
//! herald's transport behavior. Override the keepalive flags to sweep config.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use bytes::Bytes;
use clap::Parser;
use http_body_util::Empty;
use hyper::client::conn::http2;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_rustls::TlsConnector;
use url::Url;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "keepalive-probe",
    about = "Hold N idle HTTP/2 connections to an XMTP gRPC endpoint and measure their survival under a configurable keepalive (herald-lite #70)."
)]
struct Args {
    /// gRPC endpoint to dial.
    #[arg(long, default_value = "https://grpc.dev.xmtp.network:443")]
    endpoint: String,

    /// Number of concurrent idle connections to hold.
    #[arg(long, default_value_t = 1)]
    count: usize,

    /// HTTP/2 keepalive PING interval (libxmtp default: 16s).
    #[arg(long, default_value = "16s", value_parser = humantime::parse_duration)]
    ka_interval: Duration,

    /// Close the connection if no PONG arrives within this (libxmtp default: 10s).
    /// This is the #70 knob — raise it to give a slow path more slack.
    #[arg(long, default_value = "10s", value_parser = humantime::parse_duration)]
    ka_timeout: Duration,

    /// Send keepalive PINGs even with no active streams (libxmtp default: true).
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    ka_while_idle: bool,

    /// TCP + TLS connect timeout (libxmtp default: 10s).
    #[arg(long, default_value = "10s", value_parser = humantime::parse_duration)]
    connect_timeout: Duration,

    /// Stop the run after this long; survivors are reported as "alive >= duration".
    #[arg(long, default_value = "600s", value_parser = humantime::parse_duration)]
    duration: Duration,

    /// Delay between starting successive connections (avoid a boot thundering-herd).
    #[arg(long, default_value = "0ms", value_parser = humantime::parse_duration)]
    stagger: Duration,
}

#[derive(Debug, Clone)]
enum Outcome {
    /// Connection driver returned an error (keepalive timeout, GOAWAY, reset, ...).
    Died(String),
    /// Connection closed cleanly (server GOAWAY / EOF without error).
    Closed,
    /// Still alive when the run duration elapsed.
    Survived,
    /// Failed to establish (DNS / TCP / TLS / HTTP-2 handshake).
    SetupError(String),
}

struct ConnResult {
    id: usize,
    /// Connection lifetime (from handshake to death/cutoff); setup time on SetupError.
    lifetime: Duration,
    outcome: Outcome,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to install rustls ring crypto provider"))?;

    let url = Url::parse(&args.endpoint).context("parsing --endpoint")?;
    let host = url
        .host_str()
        .context("--endpoint has no host")?
        .to_string();
    let port = url.port().unwrap_or(443);

    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut tls = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"h2".to_vec()];
    let tls = Arc::new(tls);

    tracing::info!(
        endpoint = %args.endpoint,
        count = args.count,
        ka_interval = ?args.ka_interval,
        ka_timeout = ?args.ka_timeout,
        ka_while_idle = args.ka_while_idle,
        duration = ?args.duration,
        "starting keepalive probe"
    );

    let mut set: JoinSet<ConnResult> = JoinSet::new();
    for id in 0..args.count {
        let args = args.clone();
        let host = host.clone();
        let tls = tls.clone();
        let stagger = args
            .stagger
            .checked_mul(id as u32)
            .unwrap_or(Duration::ZERO);
        set.spawn(async move {
            if !stagger.is_zero() {
                tokio::time::sleep(stagger).await;
            }
            run_connection(id, &args, &host, port, tls).await
        });
    }

    let mut results: Vec<ConnResult> = Vec::with_capacity(args.count);
    loop {
        tokio::select! {
            joined = set.join_next() => match joined {
                Some(Ok(r)) => { log_result(&r); results.push(r); }
                Some(Err(e)) => tracing::error!("worker task failed to join: {e}"),
                None => break,
            },
            _ = tokio::signal::ctrl_c() => {
                tracing::warn!(remaining = set.len(), "interrupted — aborting remaining connections");
                set.abort_all();
                while let Some(joined) = set.join_next().await {
                    if let Ok(r) = joined {
                        log_result(&r);
                        results.push(r);
                    }
                }
                break;
            }
        }
    }

    summarize(&results, args.count);
    Ok(())
}

async fn run_connection(
    id: usize,
    args: &Args,
    host: &str,
    port: u16,
    tls: Arc<ClientConfig>,
) -> ConnResult {
    let start = Instant::now();
    match establish_and_hold(args, host, port, tls).await {
        Ok((outcome, lifetime)) => ConnResult {
            id,
            lifetime,
            outcome,
        },
        Err(e) => ConnResult {
            id,
            lifetime: start.elapsed(),
            outcome: Outcome::SetupError(format!("{e:#}")),
        },
    }
}

/// Establish one TLS+H2 connection, hold it idle, and await its death (or the
/// run cutoff). Returns the outcome and the connection's lifetime.
async fn establish_and_hold(
    args: &Args,
    host: &str,
    port: u16,
    tls: Arc<ClientConfig>,
) -> Result<(Outcome, Duration)> {
    let tcp = tokio::time::timeout(args.connect_timeout, TcpStream::connect((host, port)))
        .await
        .context("tcp connect timed out")?
        .context("tcp connect")?;
    tcp.set_nodelay(true).ok();

    let server_name = ServerName::try_from(host.to_string()).context("invalid TLS server name")?;
    let tls_stream = tokio::time::timeout(
        args.connect_timeout,
        TlsConnector::from(tls).connect(server_name, tcp),
    )
    .await
    .context("tls handshake timed out")?
    .context("tls handshake")?;

    let mut builder = http2::Builder::new(TokioExecutor::new());
    builder
        // hyper 1.x ships no timer; keepalive needs one.
        .timer(TokioTimer::new())
        .keep_alive_interval(args.ka_interval)
        .keep_alive_timeout(args.ka_timeout)
        .keep_alive_while_idle(args.ka_while_idle);

    let (sender, conn) = builder
        .handshake::<_, Empty<Bytes>>(TokioIo::new(tls_stream))
        .await
        .context("http2 handshake")?;

    // "Do nothing": keep the SendRequest handle alive so hyper holds the
    // connection open, send no requests, and drive the connection by awaiting
    // it. It resolves only when the connection actually dies.
    let _hold = sender;
    let connected = Instant::now();
    let outcome = match tokio::time::timeout(args.duration, conn).await {
        Err(_) => Outcome::Survived,
        Ok(Ok(())) => Outcome::Closed,
        Ok(Err(e)) => Outcome::Died(e.to_string()),
    };
    Ok((outcome, connected.elapsed()))
}

fn log_result(r: &ConnResult) {
    let life = humantime::format_duration(round_secs(r.lifetime));
    match &r.outcome {
        Outcome::Died(e) => tracing::warn!(id = r.id, %life, "DIED: {e}"),
        Outcome::Closed => tracing::info!(id = r.id, %life, "closed (graceful)"),
        Outcome::Survived => tracing::info!(id = r.id, %life, "alive at cutoff"),
        Outcome::SetupError(e) => tracing::error!(id = r.id, %life, "setup error: {e}"),
    }
}

fn summarize(results: &[ConnResult], requested: usize) {
    let died: Vec<&ConnResult> = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Died(_)))
        .collect();
    let closed = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Closed))
        .count();
    let survived = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::Survived))
        .count();
    let setup_err = results
        .iter()
        .filter(|r| matches!(r.outcome, Outcome::SetupError(_)))
        .count();

    println!("\n===== keepalive-probe summary =====");
    println!("requested:   {requested}");
    println!("established: {}", requested - setup_err);
    println!("  died:      {}", died.len());
    println!("  closed:    {closed}");
    println!("  survived:  {survived}");
    println!("setup errors:{setup_err}");

    if !died.is_empty() {
        let mut lifetimes: Vec<Duration> = died.iter().map(|r| r.lifetime).collect();
        lifetimes.sort();
        println!("\nlifetime of DIED connections:");
        println!("  min  {}", fmt(percentile(&lifetimes, 0.0)));
        println!("  p50  {}", fmt(percentile(&lifetimes, 0.50)));
        println!("  p95  {}", fmt(percentile(&lifetimes, 0.95)));
        println!("  max  {}", fmt(percentile(&lifetimes, 1.0)));

        println!("\ndeath reasons:");
        for (reason, n) in reason_histogram(&died) {
            println!("  {n:>5}  {reason}");
        }
    }
    println!("===================================");
}

/// Nearest-rank-ish percentile over a pre-sorted slice. `p` in [0.0, 1.0].
fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let rank = (p * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

/// Collapse raw error strings into a few coarse buckets, counted desc.
fn reason_histogram(died: &[&ConnResult]) -> Vec<(String, usize)> {
    use std::collections::HashMap;
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for r in died {
        if let Outcome::Died(e) = &r.outcome {
            *counts.entry(classify(e)).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(k, n)| (k.to_string(), n))
        .collect();
    v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    v
}

fn classify(err: &str) -> &'static str {
    let e = err.to_ascii_lowercase();
    if e.contains("keepalive") {
        "keepalive timeout"
    } else if e.contains("goaway") {
        "server GOAWAY"
    } else if e.contains("reset") || e.contains("rst") {
        "connection reset"
    } else if e.contains("broken pipe") {
        "broken pipe"
    } else if e.contains("timed out") || e.contains("timeout") {
        "timeout"
    } else {
        "other transport error"
    }
}

fn round_secs(d: Duration) -> Duration {
    Duration::from_secs(d.as_secs())
}

fn fmt(d: Duration) -> String {
    humantime::format_duration(round_secs(d)).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(v: &[u64]) -> Vec<Duration> {
        v.iter().map(|s| Duration::from_secs(*s)).collect()
    }

    #[test]
    fn percentile_basics() {
        let s = secs(&[1, 2, 3, 4, 5]);
        assert_eq!(percentile(&s, 0.0), Duration::from_secs(1));
        assert_eq!(percentile(&s, 0.50), Duration::from_secs(3));
        assert_eq!(percentile(&s, 0.95), Duration::from_secs(5));
        assert_eq!(percentile(&s, 1.0), Duration::from_secs(5));
    }

    #[test]
    fn percentile_empty_is_zero() {
        assert_eq!(percentile(&[], 0.5), Duration::ZERO);
    }

    #[test]
    fn classify_buckets() {
        assert_eq!(
            classify("connection error: keepalive timed out"),
            "keepalive timeout"
        );
        assert_eq!(classify("http2 error: GOAWAY"), "server GOAWAY");
        assert_eq!(classify("connection reset by peer"), "connection reset");
    }
}

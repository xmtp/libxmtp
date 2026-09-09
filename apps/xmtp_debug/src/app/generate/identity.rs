use std::{collections::HashSet, sync::Arc};

use crate::app::register_client;
use crate::app::store::{Database, IdentityStore};
use crate::app::{self, types::Identity};
use crate::metrics::{
    csv_metric, push_metrics, record_latency, record_phase_metric, record_throughput,
};

use color_eyre::eyre::{self, Result, bail, eyre};
use futures::{StreamExt, TryFutureExt, TryStreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::time::Instant;

/// Identity Generation
pub struct GenerateIdentity {
    identity_store: IdentityStore<'static>,
}

impl GenerateIdentity {
    pub fn new(identity_store: IdentityStore<'static>) -> Self {
        Self { identity_store }
    }

    pub async fn create_identities(&self, n: usize, concurrency: usize) -> Result<Vec<Identity>> {
        let loop_pause_secs: Option<u64> = std::env::var("XDBG_LOOP_PAUSE")
            .ok()
            .and_then(|v| v.parse().ok());

        let style = ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} | {msg}");
        let bar = ProgressBar::new(n as u64)
            .with_style(style.unwrap())
            .with_message("generating identities");
        // simple task to keep the bar elapsed time moving
        tokio::spawn({
            let b = bar.clone();
            async move {
                let s = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
                    std::time::Duration::from_millis(100),
                ));
                futures::pin_mut!(s);
                while s.next().await.is_some() {
                    b.tick();
                }
            }
        });

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

        tracing::info!("creating clients");
        let clients = stream::iter((0..n).collect::<Vec<_>>())
            .map(|_| {
                tokio::spawn({
                    let sem = semaphore.clone();
                    let bar_pointer = bar.clone();
                    async move {
                        let _permit = sem.acquire().await?;
                        let wallet = crate::app::generate_wallet();
                        let t_init = Instant::now();
                        let c = app::new_unregistered_client(Some(&wallet)).await?;
                        let init_secs = t_init.elapsed().as_secs_f64();

                        record_phase_metric(
                            "identity_client_init",
                            init_secs,
                            "client_init",
                            "xdbg_debug",
                        )
                        .await;

                        bar_pointer
                            .set_message(format!("generated client {}", c.identity().inbox_id()));
                        bar_pointer.inc(1);
                        Ok::<_, eyre::Report>((c, wallet))
                    }
                })
                .map_err(|_| eyre!("failed to create client"))
            })
            .buffer_unordered(concurrency)
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        bar.finish();
        bar.reset();

        let identities = stream::iter(clients.into_iter().map(Ok))
            .map_ok(|(c, wallet)| {
                tokio::spawn({
                    let sem = semaphore.clone();
                    async move {
                        let _permit = sem.acquire().await?;
                        let identity = Identity::from_libxmtp(c.identity(), wallet.clone())?;
                        let t_register = Instant::now();
                        register_client(&c, wallet.into_alloy()).await?;
                        let register_secs = t_register.elapsed().as_secs_f64();

                        record_phase_metric(
                            "identity_register",
                            register_secs,
                            "register",
                            "xdbg_debug",
                        )
                        .await;

                        Ok(identity)
                    }
                })
                .map_err(|_| eyre!("failed to register identities"))
            })
            .try_buffer_unordered(concurrency)
            .map(|j| j.flatten())
            .try_collect::<Vec<Identity>>()
            .await?;

        self.identity_store.set_all(identities.as_slice())?;

        let tmp = Arc::new(app::temp_client(None).await?);
        let states = stream::iter(identities.iter().copied().map(Ok))
            .map_ok(|identity| {
                let tmp = tmp.clone();
                tokio::spawn({
                    let sem = semaphore.clone();
                    async move {
                        let _permit = sem.acquire().await?;
                        let inbox_id_hex = hex::encode(identity.inbox_id);
                        trace!(inbox_id = inbox_id_hex, "getting association state");

                        poll_association_readiness(&inbox_id_hex).await?;

                        measure_sync_and_lookup(&identity, &tmp, &inbox_id_hex).await?;

                        // -- XDBG_LOOP_PAUSE --
                        if let Some(secs) = loop_pause_secs {
                            tracing::debug!(secs, "sleeping XDBG_LOOP_PAUSE after identity");
                            tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                        }

                        Ok(())
                    }
                })
                .map_err(|_| eyre!("failed to register identities"))
            })
            .try_buffer_unordered(concurrency)
            .map(|j| j.flatten())
            .collect::<Vec<_>>()
            .await;
        let errs = states
            .into_iter()
            .filter_map(|s| s.err())
            .map(|e| e.to_string())
            .collect::<Vec<String>>();
        let unique: HashSet<String> = HashSet::from_iter(errs.clone());
        if !unique.is_empty() {
            tracing::error!("{} errors during identity generation", errs.len());
            tracing::error!("{} unique errors during identity generation", unique.len());
            for err in unique.into_iter() {
                error!(error = err);
            }
            bail!("Error generation failed");
        }

        // -- verify all identities are readable from a fresh temp client --
        verify_identities_readable(&identities).await?;

        Ok(identities)
    }
}

/// Poll until the identity's association state has members, or timeout after 30s.
async fn poll_association_readiness(inbox_id_hex: &str) -> Result<()> {
    let reader = Arc::new(app::temp_client(None).await?);
    let conn = Arc::new(reader.context.store().db());

    let assoc_start = Instant::now();
    let assoc_deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(30);
    let poll_interval = tokio::time::Duration::from_millis(50);
    let mut assoc_ready = false;

    loop {
        let state = reader
            .identity_updates()
            .get_latest_association_state(&conn, inbox_id_hex)
            .await?;
        if !state.members().is_empty() {
            assoc_ready = true;
            break;
        }
        if tokio::time::Instant::now() >= assoc_deadline {
            break;
        }
        tokio::time::sleep(poll_interval).await;
    }
    let assoc_secs = assoc_start.elapsed().as_secs_f64();
    let assoc_ok = if assoc_ready { "true" } else { "false" };

    record_latency("identity_assoc_ready", assoc_secs);
    record_throughput("identity_assoc_ready");
    csv_metric(
        "latency_seconds",
        "identity_assoc_ready",
        assoc_secs,
        &[("phase", "assoc_ready"), ("success", assoc_ok)],
    );
    csv_metric(
        "throughput_events",
        "identity_assoc_ready",
        1.0,
        &[("phase", "assoc_ready"), ("success", assoc_ok)],
    );
    push_metrics("xdbg_debug").await;

    Ok(())
}

/// Measure welcome-sync latency and identity-lookup latency for a registered identity.
async fn measure_sync_and_lookup(
    identity: &Identity,
    tmp: &crate::DbgClient,
    inbox_id_hex: &str,
) -> Result<()> {
    let conn = Arc::new(tmp.context.store().db());

    // -- welcome sync latency --
    let t_sync = Instant::now();
    let c = app::client_from_identity(identity)?;
    c.sync_welcomes().await?;
    let sync_secs = t_sync.elapsed().as_secs_f64();

    record_phase_metric(
        "identity_read_sync_latency",
        sync_secs,
        "identity_read_sync",
        "xdbg_debug",
    )
    .await;

    // -- identity lookup latency --
    let t_lookup = Instant::now();
    let _ = tmp
        .identity_updates()
        .get_latest_association_state(&conn, inbox_id_hex)
        .await?;
    let lookup_secs = t_lookup.elapsed().as_secs_f64();

    record_phase_metric(
        "read_identity_lookup_latency",
        lookup_secs,
        "identity_read",
        "xdbg_debug",
    )
    .await;

    Ok(())
}

/// Verify all identities are readable from a fresh temp client.
async fn verify_identities_readable(identities: &[Identity]) -> Result<()> {
    let verify_client = Arc::new(app::temp_client(None).await?);
    let verify_conn = Arc::new(verify_client.context.store().db());
    for identity in identities {
        let inbox_id_hex = hex::encode(identity.inbox_id);
        let t_verify = Instant::now();
        let _ = verify_client
            .identity_updates()
            .get_latest_association_state(&verify_conn, &inbox_id_hex)
            .await?;
        let verify_secs = t_verify.elapsed().as_secs_f64();

        record_phase_metric(
            "verify_identity_lookup_latency",
            verify_secs,
            "verify_identity_read",
            "xdbg_debug",
        )
        .await;
    }
    Ok(())
}

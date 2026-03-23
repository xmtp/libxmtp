use std::{collections::HashSet, sync::Arc};

use crate::app::register_client;
use crate::app::store::{Database, IdentityStore};
use crate::app::{self, types::Identity};
use crate::args;
use crate::metrics::{
    csv_metric, push_metrics, record_latency, record_phase_metric, record_throughput,
};

use color_eyre::eyre::{self, Result, WrapErr, bail, eyre};
use futures::{FutureExt, StreamExt, TryFutureExt, TryStreamExt, future, stream};
use indicatif::{ProgressBar, ProgressStyle};
use openmls_rust_crypto::RustCrypto;
use tokio::sync::Mutex;
use tokio::time::{Instant, timeout};
use xmtp_api_d14n::d14n::SubscribeTopics;
use xmtp_api_d14n::protocol::{CollectionExtractor, Extractor, KeyPackagesExtractor};
use xmtp_proto::api::QueryStreamExt;
use xmtp_proto::types::{InstallationId, TopicCursor, TopicKind};
use xmtp_proto::xmtp::xmtpv4::message_api::subscribe_topics_response::Response as SubscribeTopicsResponse;

/// Identity Generation
pub struct GenerateIdentity {
    identity_store: IdentityStore<'static>,
    network: args::BackendOpts,
}

impl GenerateIdentity {
    pub fn new(identity_store: IdentityStore<'static>, network: args::BackendOpts) -> Self {
        Self {
            identity_store,
            network,
        }
    }

    pub async fn create_identities(
        &self,
        n: usize,
        concurrency: usize,
        ryow: bool,
    ) -> Result<Vec<Identity>> {
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
        let network = &self.network;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let s = Arc::new(Mutex::new(TopicCursor::default()));

        tracing::info!("creating clients");
        let clients = stream::iter((0..n).collect::<Vec<_>>())
            .map(|_| {
                tokio::spawn({
                    let sem = semaphore.clone();
                    let s = s.clone();
                    let network = network.clone();
                    let bar_pointer = bar.clone();
                    async move {
                        let _permit = sem.acquire().await?;
                        let wallet = crate::app::generate_wallet();
                        let t_init = Instant::now();
                        let c = app::new_unregistered_client(&network, Some(&wallet)).await?;
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
                        let mut s = s.lock().await;
                        s.add(
                            TopicKind::KeyPackagesV1.create(c.identity().installation_id()),
                            Default::default(),
                        );
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

        let topic_cursor =
            Arc::into_inner(s).expect("only one reference exists after tasks finish");
        let topic_cursor = Mutex::into_inner(topic_cursor);
        // try to read a key package for each installation id we created
        // only for D14n
        let (tx, rx) = tokio::sync::oneshot::channel();
        let read_writes = if network.d14n && ryow {
            let n = network.clone();
            let mut needed_installations = clients
                .clone()
                .iter()
                .map(|(c, _)| c.context.installation_id())
                .collect::<HashSet<_>>();
            tokio::spawn(async move {
                let api = n.xmtpd()?;
                let mut s = SubscribeTopics::builder().topics(topic_cursor).build()?;
                let s = s.subscribe(&api).await?;
                let bar_ref = bar.clone();
                let _ = tx.send(());
                bar_ref.set_message("waiting for identities to be written");
                futures::pin_mut!(s);
                while let Some(kp) = timeout(n.ryow_timeout.into(), s.try_next())
                    .await
                    .wrap_err("timeout reached for reading writes on key package published")??
                {
                    let envelopes = match kp.response {
                        Some(SubscribeTopicsResponse::Envelopes(e)) => e.envelopes,
                        _ => continue,
                    };
                    // TODO: we can deserialize key packages in extractors possibly
                    let extractor =
                        CollectionExtractor::new(envelopes, KeyPackagesExtractor::new());
                    let key_packages = extractor.get()?;
                    let key_packages = key_packages
                        .into_iter()
                        .map(|kp| {
                            let inst = xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes(
                                &RustCrypto::default(),
                                kp.key_package_tls_serialized.as_slice(),
                            )?
                            .installation_public_key;
                            Ok(InstallationId::try_from(inst)?)
                        })
                        .inspect(|v| {
                            let _ = v.as_ref().inspect(|v| {
                                bar_ref
                                    .set_message(format!("got key package for installation {}", v));
                            });
                        })
                        .collect::<Result<HashSet<_>, eyre::Report>>()?;
                    bar.inc(key_packages.len() as u64);
                    needed_installations = needed_installations
                        .difference(&key_packages)
                        .copied()
                        .collect();
                    if needed_installations.is_empty() {
                        break;
                    }
                }
                Ok(())
            })
            .map_err(|_| eyre!("failed to read own writes"))
            .map(|s| s.flatten())
            .boxed()
        } else {
            let _ = tx.send(());
            future::ready(Ok(())).boxed()
        };
        // ensure our ryow task is spawned
        let _ = rx.await;

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

        self.identity_store
            .set_all(identities.as_slice(), &self.network)?;

        //TODO: this can be removed once we're d14n-only
        let tmp = Arc::new(app::temp_client(network, None).await?);
        let states = stream::iter(identities.iter().copied().map(Ok))
            .map_ok(|identity| {
                let tmp = tmp.clone();
                tokio::spawn({
                    let sem = semaphore.clone();
                    let network_clone = network.clone();
                    async move {
                        let _permit = sem.acquire().await?;
                        let inbox_id_hex = hex::encode(identity.inbox_id);
                        trace!(inbox_id = inbox_id_hex, "getting association state");

                        poll_association_readiness(&network_clone, &inbox_id_hex).await?;

                        measure_sync_and_lookup(&network_clone, &identity, &tmp, &inbox_id_hex)
                            .await?;

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
                error!(err);
            }
            bail!("Error generation failed");
        }

        // -- verify all identities are readable from a fresh temp client --
        verify_identities_readable(network, &identities).await?;

        read_writes.await?;
        Ok(identities)
    }
}

/// Poll until the identity's association state has members, or timeout after 30s.
async fn poll_association_readiness(network: &args::BackendOpts, inbox_id_hex: &str) -> Result<()> {
    let reader = Arc::new(app::temp_client(network, None).await?);
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
    network: &args::BackendOpts,
    identity: &Identity,
    tmp: &crate::DbgClient,
    inbox_id_hex: &str,
) -> Result<()> {
    let conn = Arc::new(tmp.context.store().db());

    // -- welcome sync latency --
    let t_sync = Instant::now();
    let c = app::client_from_identity(identity, network)?;
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
async fn verify_identities_readable(
    network: &args::BackendOpts,
    identities: &[Identity],
) -> Result<()> {
    let verify_client = Arc::new(app::temp_client(network, None).await?);
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

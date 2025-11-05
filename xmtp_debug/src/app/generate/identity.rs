use std::{collections::HashSet, sync::Arc};

use crate::app::register_client;
use crate::app::store::{Database, IdentityStore};
use crate::app::{self, types::Identity};
use crate::args;

use color_eyre::eyre::{self, Result, WrapErr, bail, eyre};
use futures::{FutureExt, StreamExt, TryFutureExt, TryStreamExt, future, stream};
use indicatif::{ProgressBar, ProgressStyle};
use openmls_rust_crypto::RustCrypto;
use tokio::sync::Mutex;
use tokio::time::timeout;
use xmtp_api_d14n::d14n::SubscribeEnvelopes;
use xmtp_api_d14n::protocol::{CollectionExtractor, Extractor, KeyPackagesExtractor};
use xmtp_proto::api::QueryStreamExt;
use xmtp_proto::types::{InstallationId, TopicKind};

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

    #[allow(unused)]
    pub fn load_identities(
        &self,
    ) -> Result<Option<impl Iterator<Item = Result<Identity>> + use<'_>>> {
        Ok(self
            .identity_store
            .load(&self.network)?
            .map(|i| i.map(|i| Ok(i.value()))))
    }

    pub async fn create_identities(&self, n: usize, concurrency: usize) -> Result<Vec<Identity>> {
        let style = ProgressStyle::with_template("{bar} {pos}/{len} elapsed {elapsed} | {msg}");
        let bar = ProgressBar::new(n as u64)
            .with_style(style.unwrap())
            .with_message("generating identities");
        tokio::spawn({
            let b = bar.clone();
            async move {
                let s = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(
                    std::time::Duration::from_millis(100),
                ));
                futures::pin_mut!(s);
                while let Some(_) = s.next().await {
                    b.tick();
                }
            }
        });
        let network = &self.network;

        let s = Arc::new(Mutex::new(SubscribeEnvelopes::builder()));

        tracing::info!("creating clients");
        let clients = stream::iter((0..n).collect::<Vec<_>>())
            .map(|_| {
                tokio::spawn({
                    let s = s.clone();
                    let network = network.clone();
                    let bar_pointer = bar.clone();
                    async move {
                        let wallet = crate::app::generate_wallet();
                        let c = app::new_unregistered_client(&network, Some(&wallet)).await?;
                        bar_pointer
                            .set_message(format!("generated client {}", c.identity().inbox_id()));
                        let mut s = s.lock().await;
                        s.topic(TopicKind::KeyPackagesV1.create(c.identity().installation_id()));
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

        let s = Arc::into_inner(s).expect("only one reference exists after tasks finish");
        let mut s = Mutex::into_inner(s);
        // try to read a key package for each installation id we created
        // only for D14n
        let (tx, rx) = tokio::sync::oneshot::channel();
        let read_writes = if network.d14n {
            let n = network.clone();
            let mut needed_installations = clients
                .clone()
                .iter()
                .map(|(c, _)| c.context.installation_id())
                .collect::<HashSet<_>>();
            tokio::spawn(async move {
                let api = n.xmtpd()?;
                let mut s = s.last_seen(None).build()?;
                let s = s.subscribe(&api).await?;
                let bar_ref = bar.clone();
                let _ = tx.send(());
                bar_ref.set_message("waiting for identities to be written");
                futures::pin_mut!(s);
                while let Some(kp) = timeout(n.ryow_timeout.into(), s.try_next()).await.wrap_err("timeout reached for reading writes on key package published")?? {
                    // TODO: we can deserialize key packages in extractors possibly
                    let extractor =
                        CollectionExtractor::new(kp.envelopes, KeyPackagesExtractor::new());
                    let key_packages = extractor.get()?;
                    let key_packages = key_packages
                        .into_iter()
                        .map(|kp| {
                            let inst = xmtp_mls::verified_key_package_v2::VerifiedKeyPackageV2::from_bytes(
                                &RustCrypto::default(),
                                kp.key_package_tls_serialized.as_slice(),
                            )?
                            .installation_public_key;
                            Ok(InstallationId::try_from(inst)?)
                        })
                        .inspect(|v| { let _ = v.as_ref().inspect(|v| { bar_ref.set_message(format!("got key package for installation {}", v)); }); })
                        .collect::<Result<HashSet<_>, eyre::Report>>()?;
                    bar.inc(key_packages.len() as u64);
                    needed_installations = needed_installations.difference(&key_packages).copied().collect();
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
                tokio::spawn(async move {
                    let identity = Identity::from_libxmtp(c.identity(), wallet.clone())?;
                    register_client(&c, wallet.into_alloy()).await?;
                    Ok(identity)
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
        let conn = Arc::new(tmp.context.store().db());
        let states = stream::iter(identities.iter().copied().map(Ok))
            .map_ok(|identity| {
                let tmp = tmp.clone();
                let conn = conn.clone();
                tokio::spawn(async move {
                    let id = hex::encode(identity.inbox_id);
                    trace!(inbox_id = id, "getting association state");
                    let state = tmp
                        .identity_updates()
                        .get_latest_association_state(&conn, &id)
                        .await?;
                    Ok(state)
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
        read_writes.await?;
        Ok(identities)
    }
}

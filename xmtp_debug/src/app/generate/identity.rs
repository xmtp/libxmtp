use std::{collections::HashSet, sync::Arc};

use crate::app::register_client;
use crate::app::store::{Database, IdentityStore};
use crate::app::{self, types::Identity};
use crate::args;
use crate::queries::key_packages;

use color_eyre::eyre::{self, Result, ensure, eyre};
use futures::{FutureExt, StreamExt, TryFutureExt, TryStreamExt, future, stream};
use indicatif::{ProgressBar, ProgressStyle};
use openmls_rust_crypto::RustCrypto;
use tokio::sync::Mutex;
use tokio::time::timeout;
use xmtp_api_d14n::d14n::SubscribeEnvelopes;
use xmtp_api_d14n::protocol::{CollectionExtractor, Extractor, KeyPackagesExtractor};
use xmtp_cryptography::XmtpInstallationCredential;
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

    pub async fn create_identities(
        &self,
        n: usize,
        concurrency: usize,
        ryow: bool,
    ) -> Result<Vec<Identity>> {
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
        let s = Arc::new(Mutex::new(SubscribeEnvelopes::builder()));

        tracing::info!("creating clients");
        let clients: Vec<_> = stream::iter((0..n).collect::<Vec<_>>())
            .map(|_| {
                tokio::spawn({
                    let sem = semaphore.clone();
                    let s = s.clone();
                    let network = network.clone();
                    let bar_pointer = bar.clone();
                    async move {
                        let _permit = sem.acquire().await?;
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
        let read_writes = if network.d14n && ryow {
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
                while let Some(kp) = timeout(n.ryow_timeout.into(), s.try_next()).await?? {
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

        let c = clients.clone();
        let identities = stream::iter(c.into_iter().map(Ok))
            .map_ok(|(c, wallet)| {
                tokio::spawn({
                    let sem = semaphore.clone();
                    let wallet = wallet.clone();
                    async move {
                        let _permit = sem.acquire().await;
                        let identity = Identity::from_libxmtp(c.identity(), wallet.clone())?;
                        register_client(&c, wallet.into_alloy()).await?;
                        Ok(identity)
                    }
                })
                .map_err(|_| eyre!("failed to register identities"))
            })
            .try_buffer_unordered(concurrency)
            .map(|j| j.flatten())
            .try_collect::<Vec<Identity>>()
            .await?;

        let original_ids = identities.clone();
        self.identity_store
            .set_all(identities.as_slice(), &self.network)?;
        // since we cannot depend on RYOW and KPS dont get uploaded
        // just keep trying to upload until they do.
        // TODO:d14n Hopefully can remove this at some point
        let key_package_keys: HashSet<Vec<u8>> = key_packages(&self.network, identities.iter())
            .await?
            .iter()
            .map(|v| v.installation_public_key.clone())
            .collect();
        let mut needed_keys: HashSet<Vec<u8>> = identities
            .iter()
            .map(|i| {
                let cred = XmtpInstallationCredential::from_bytes(&i.installation_key).unwrap();
                cred.public_bytes().to_vec()
            })
            .collect();
        needed_keys = needed_keys.difference(&key_package_keys).cloned().collect();
        while !needed_keys.is_empty() {
            tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            tracing::info!("still need {}", needed_keys.len());
            stream::iter(clients.iter().map(Ok))
                .try_for_each(async |c| {
                    if needed_keys.contains(c.0.context.installation_id().as_ref()) {
                        c.0.rotate_and_upload_key_package().await?;
                    }
                    Ok::<_, eyre::Report>(())
                })
                .await?;
            let check_ids = original_ids.iter().filter(|id| {
                let key = XmtpInstallationCredential::from_bytes(&id.installation_key).unwrap();
                needed_keys.contains(&key.public_bytes().to_vec())
            });
            let key_package_keys: HashSet<Vec<u8>> = key_packages(&self.network, check_ids)
                .await?
                .iter()
                .map(|v| v.installation_public_key.clone())
                .collect();
            needed_keys = needed_keys.difference(&key_package_keys).cloned().collect();
        }
        read_writes.await?;

        let key_package_keys: Vec<_> = key_packages(&self.network, original_ids.iter()).await?;
        ensure!(
            key_package_keys.len() == original_ids.len(),
            "created {} identities, but only {} key packages were uploaded",
            identities.len(),
            key_package_keys.len()
        );
        Ok(identities)
    }
}

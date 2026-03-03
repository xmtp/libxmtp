//! Group Generation
use crate::app::identity_lock::get_identity_lock;
use crate::app::load_n_identities;
use crate::app::{
    store::{Database, GroupStore, IdentityStore, RandomDatabase},
    types::*,
};
use crate::args;
use crate::metrics::{csv_metric, push_metrics, record_latency, record_member_count, record_phase_metric, record_throughput};
use color_eyre::eyre::{self, Result, ensure, eyre};
use futures::{StreamExt, TryFutureExt, TryStreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use std::time::Instant;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_proto::types::InstallationId;

pub struct GenerateGroups {
    group_store: GroupStore<'static>,
    identity_store: IdentityStore<'static>,
    network: args::BackendOpts,
}

impl GenerateGroups {
    pub fn new(db: Arc<redb::Database>, network: args::BackendOpts) -> Self {
        Self {
            group_store: db.clone().into(),
            identity_store: db.clone().into(),
            network,
        }
    }

    pub async fn create_groups(
        &self,
        n: usize,
        invitees: usize,
        concurrency: usize,
    ) -> Result<Vec<Group>> {
        tracing::info!("creating groups");

        let network = &self.network;
        let identities = self.identity_store.len(network)?;
        ensure!(
            identities >= invitees,
            "not enough identities generated. have {}, but need to invite {}. groups cannot hold duplicate identities",
            identities,
            invitees
        );
        let style = ProgressStyle::with_template(
            "{bar} {pos}/{len} elapsed {elapsed} remaining {eta_precise}",
        );
        let bar = ProgressBar::new(n as u64).with_style(style.unwrap());

        let clients = load_n_identities(&self.identity_store, network, n)?;

        // Read XDBG_LOOP_PAUSE once; tasks capture the value by copy.
        let loop_pause_secs = std::env::var("XDBG_LOOP_PAUSE")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let groups = stream::iter(clients.iter())
            .map(|(owner, client)| {
                let bar_pointer = bar.clone();
                let client = client.clone();
                let owner = *owner;
                let store = self.identity_store.clone();
                let network_hash = u64::from(network);
                let network_opts = network.clone();
                let semaphore = semaphore.clone();
                Ok(tokio::spawn({
                    async move {
                        let _permit = semaphore.acquire().await?;

                        // Acquire per-identity lock before touching the owner's MLS state.
                        let id_lock = get_identity_lock(&owner)?;
                        let _id_guard = id_lock.lock().await;

                        debug!(owner = hex::encode(owner), "group owner");
                        let invite_identities = {
                            let mut rng = rand::thread_rng();
                            store.random_n_capped(network_hash, &mut rng, invitees)
                        }?;
                        let mut ids = Vec::with_capacity(invite_identities.len());
                        for member in &invite_identities {
                            let member = member.value();
                            let cred =
                                XmtpInstallationCredential::from_bytes(&member.installation_key)?;
                            let inbox_id = hex::encode(member.inbox_id);
                            tracing::debug!(
                                inbox_ids = hex::encode(member.inbox_id),
                                installation_key = %InstallationId::from(*cred.public_bytes()),
                                "Adding Members"
                            );
                            ids.push(inbox_id);
                        }

                        let client = client.lock().await;

                        // -- group create (client-only) latency --
                        let t_total = Instant::now();
                        let t_create = Instant::now();
                        let group = client.create_group(Default::default(), Default::default())?;
                        let create_secs = t_create.elapsed().as_secs_f64();

                        record_phase_metric("group_create_client_only", create_secs, "group_create", "xdbg_debug");

                        // -- add members latency --
                        let member_count = ids.len();
                        let t_add = Instant::now();
                        group.add_members_by_inbox_id(ids.as_slice()).await?;
                        let add_secs = t_add.elapsed().as_secs_f64();

                        record_latency("group_add_members", add_secs);
                        record_member_count("group_add_members", member_count as f64);
                        record_throughput("group_add_members");
                        csv_metric(
                            "latency_seconds",
                            "group_add_members",
                            add_secs,
                            &[("phase", "add_members")],
                        );
                        csv_metric(
                            "event",
                            "group_add_members_per_member",
                            member_count as f64,
                            &[("phase", "add_members")],
                        );
                        push_metrics("xdbg_debug");

                        // -- reader-side visibility check (first invitee polls for welcome) --
                        if !invite_identities.is_empty() {
                            let first_invitee = invite_identities[0].value();
                            let reader_result = crate::app::client_from_identity(&first_invitee, &network_opts);
                            if let Ok(reader_client) = reader_result {
                                let t_visibility = Instant::now();
                                let visibility_timeout = tokio::time::Duration::from_secs(15);
                                let deadline = tokio::time::Instant::now() + visibility_timeout;
                                let poll_interval = tokio::time::Duration::from_millis(10);
                                let mut visible = false;

                                loop {
                                    if let Err(e) = reader_client.sync_welcomes().await {
                                        tracing::warn!(error = %e, "sync_welcomes failed during visibility poll");
                                    }
                                    if reader_client.group(&group.group_id).is_ok() {
                                        visible = true;
                                        break;
                                    }
                                    if tokio::time::Instant::now() >= deadline {
                                        break;
                                    }
                                    tokio::time::sleep(poll_interval).await;
                                }
                                let visibility_secs = t_visibility.elapsed().as_secs_f64();
                                let vis_ok = if visible { "1" } else { "0" };

                                // Note: record_latency/record_member_count are label-agnostic;
                                // the `success` dimension is only tracked in CSV (see csv_metric below).
                                record_latency("read_group_sync_latency", visibility_secs);
                                record_member_count("read_member_visibility", if visible { 1.0 } else { 0.0 });
                                csv_metric(
                                    "event",
                                    "read_member_visibility",
                                    if visible { 1.0 } else { 0.0 },
                                    &[("phase", "member_visibility"), ("success", vis_ok)],
                                );
                                csv_metric(
                                    "latency_seconds",
                                    "read_group_sync_latency",
                                    visibility_secs,
                                    &[("phase", "member_visibility"), ("success", vis_ok)],
                                );
                                push_metrics("xdbg_debug");
                            }
                        }

                        // -- total group create + add latency --
                        let total_secs = t_total.elapsed().as_secs_f64();
                        record_phase_metric("group_create_with_members", total_secs, "group_total", "xdbg_debug");

                        bar_pointer.inc(1);

                        let mut members = invite_identities
                            .into_iter()
                            .map(|i| i.value().inbox_id)
                            .collect::<Vec<InboxId>>();
                        members.push(owner);

                        // -- XDBG_LOOP_PAUSE --
                        if let Some(secs) = loop_pause_secs {
                            tracing::debug!(secs, "sleeping XDBG_LOOP_PAUSE after group");
                            tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                        }

                        Ok(Group {
                            id: group
                                .group_id
                                .try_into()
                                .expect("Group id expected to be 32 bytes"),
                            member_size: members.len() as u32,
                            members,
                            created_by: owner,
                        })
                    }
                })
                .map_err(|_| eyre!("failed to spawn tasks for group creation")))
            })
            .try_buffer_unordered(concurrency)
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .collect::<Result<Vec<_>, eyre::Report>>()?;
        self.group_store.set_all(groups.as_slice(), &self.network)?;
        // ensure cleanup for each client
        for client in clients.values() {
            let client = client.lock().await;
            client.release_db_connection()?;
        }
        Ok(groups)
    }
}

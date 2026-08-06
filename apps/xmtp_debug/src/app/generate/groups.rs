//! Group Generation
use crate::app::load_n_identities;
use crate::app::{
    self,
    store::{Database, GroupStore, IdentityStore},
    types::*,
};
use crate::metrics::{
    csv_metric, push_metrics, record_latency, record_member_count, record_phase_metric,
    record_throughput,
};
use color_eyre::eyre::{self, Result, ensure, eyre};
use futures::{StreamExt, TryFutureExt, TryStreamExt, stream};
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Instant;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_proto::types::{GroupId, InstallationId};

pub struct GenerateGroups {
    group_store: GroupStore<'static>,
    identity_store: IdentityStore<'static>,
}

impl GenerateGroups {
    pub fn new(db: Arc<redb::Database>) -> Self {
        Self {
            group_store: db.clone().into(),
            identity_store: db.clone().into(),
        }
    }

    pub async fn create_groups(
        &self,
        n: usize,
        invitees: usize,
        concurrency: usize,
    ) -> Result<Vec<Group>> {
        tracing::info!("creating groups");

        let loop_pause_secs: Option<u64> = std::env::var("XDBG_LOOP_PAUSE")
            .ok()
            .and_then(|v| v.parse().ok());

        let identities = self.identity_store.len()?;
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

        let clients = load_n_identities(&self.identity_store, n).await?;

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
        let groups = stream::iter(clients.iter())
            .map(|(owner, client)| {
                let bar_pointer = bar.clone();
                let client = client.clone();
                let owner = *owner;
                let store = self.identity_store.clone();
                let semaphore = semaphore.clone();
                Ok(tokio::spawn(Box::pin({
                    async move {
                        let _permit = semaphore.acquire().await?;
                        let t_total = Instant::now();

                        let (invitee_inboxes, invitee_hex, first_invitee) =
                            resolve_invitees(&store, invitees, owner)?;

                        let (group_id, create_secs) =
                            create_group_on_network(&client, &invitee_hex).await?;

                        record_phase_metric(
                            "group_create_client_only",
                            create_secs,
                            "group_create",
                            "xdbg_debug",
                        )
                        .await;

                        if let Some(ref invitee) = first_invitee
                            && let Err(e) = check_member_visibility(&group_id, invitee).await
                        {
                            if crate::fail_fast() {
                                return Err(e);
                            }
                            tracing::warn!(error = %e, "member visibility check failed");
                        }

                        // -- total group create + add latency --
                        let total_secs = t_total.elapsed().as_secs_f64();
                        record_phase_metric(
                            "group_create_with_members",
                            total_secs,
                            "group_total",
                            "xdbg_debug",
                        )
                        .await;

                        bar_pointer.inc(1);

                        // -- XDBG_LOOP_PAUSE --
                        if let Some(secs) = loop_pause_secs {
                            tracing::debug!(secs, "sleeping XDBG_LOOP_PAUSE after group");
                            tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
                        }

                        // `Group::add_members` dedupes, so even if an
                        // upstream filter slips up the owner can't appear
                        // twice in the persisted member list.
                        Ok(Group::new(group_id, owner, vec![owner]).add_members(invitee_inboxes))
                    }
                }))
                .map_err(|_| eyre!("failed to spawn tasks for group creation")))
            })
            .try_buffer_unordered(concurrency)
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .collect::<Result<Vec<_>, eyre::Report>>()?;
        self.group_store.set_all(groups.as_slice())?;
        // ensure cleanup for each client
        for client in clients.values() {
            let client = client.lock().await;
            client.release_db_connection()?;
        }
        Ok(groups)
    }
}

/// Resolve random invitees from the identity store. Returns the raw
/// inbox bytes (for redb persistence), the hex-encoded inbox ids (for
/// libxmtp's `add_members`), and optionally the first invitee's raw
/// identity. Resolved eagerly so no non-Send rng is held across await
/// points.
fn resolve_invitees(
    store: &IdentityStore<'static>,
    invitees: usize,
    owner: InboxId,
) -> Result<(Vec<InboxId>, Vec<String>, Option<Identity>)> {
    let mut rng = rand::rng();
    // Over-sample by 1 so dropping the owner (if it lands in the
    // sample) doesn't leave us short. `sample_n` honors
    // `--strict-versioning`, so cross-version inboxes are filtered at
    // the store level — no need to re-filter here.
    let sample = store.sample_n(&mut rng, invitees + 1, crate::app::App::strict_versioning())?;
    let mut iter = sample
        .into_iter()
        .filter(|id| id.inbox_id != owner)
        .take(invitees);

    let first_invitee = iter.next();
    let mut inboxes = Vec::with_capacity(invitees);
    let mut hex_ids = Vec::with_capacity(invitees);
    for member in first_invitee.into_iter().chain(iter) {
        let cred = XmtpInstallationCredential::from_bytes(&member.installation_key)?;
        tracing::debug!(
            inbox_id = hex::encode(member.inbox_id),
            installation_key = %InstallationId::from(*cred.public_bytes()),
            "Adding Members"
        );
        inboxes.push(member.inbox_id);
        hex_ids.push(hex::encode(member.inbox_id));
    }
    debug!(
        owner = hex::encode(owner),
        member_count = hex_ids.len(),
        "group owner"
    );
    Ok((inboxes, hex_ids, first_invitee))
}

/// Create a group and add members, returning the group ID and the group-creation latency.
async fn create_group_on_network(
    client: &Mutex<crate::DbgClient>,
    member_ids: &[String],
) -> Result<(GroupId, f64)> {
    let t_create = Instant::now();
    let client_guard = client.lock().await;
    let group = client_guard.create_group(Default::default(), Default::default())?;
    let create_secs = t_create.elapsed().as_secs_f64();

    let t_add = Instant::now();
    group.add_members(member_ids).await?;
    let add_secs = t_add.elapsed().as_secs_f64();

    record_latency("group_add_members", add_secs);
    record_member_count("group_add_members", member_ids.len() as f64);
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
        member_ids.len() as f64,
        &[("phase", "add_members")],
    );
    push_metrics("xdbg_debug").await;

    drop(client_guard);

    Ok((group.group_id, create_secs))
}

/// Check whether an invitee can see the group via sync_welcomes (read-your-own-writes).
async fn check_member_visibility(group_id: &GroupId, invitee: &Identity) -> Result<()> {
    let reader_client = app::client_from_identity(invitee)?;

    let t_visibility = Instant::now();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(30);
    let poll_interval = tokio::time::Duration::from_millis(100);
    let mut visible = false;

    loop {
        reader_client.sync_welcomes().await?;
        if reader_client.group(group_id).is_ok() {
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
    push_metrics("xdbg_debug").await;
    Ok(())
}

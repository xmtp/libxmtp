//! Repeatable stream validation with independent persistent peers.

use std::{path::PathBuf, time::Duration};

use color_eyre::eyre::{Result, WrapErr, bail, eyre};
use futures::{StreamExt, future::try_join_all};
use openmls::group::MlsGroup as OpenMlsGroup;
use xmtp_api::PublishUnit;
use xmtp_db::{
    NotFound, StorageError,
    TransactionOutcome::Continue,
    TransactionalKeyStore, XmtpMlsStorageProvider,
    group_message::MsgQueryArgs,
    incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
};
use xmtp_mls::{
    context::XmtpSharedContext,
    groups::MlsGroup,
    subscriptions::{barrier::wait_through, stream_messages::StreamGroupMessages},
};
use xmtp_proto::{
    backend_v1::client_envelope::Payload,
    types::{Cursor, GroupId, Topic},
};

use super::{App, clients, generate_wallet};
use crate::{
    DbgClient, MlsContext,
    args::{BackendOpts, TestOpts},
};

mod proxy;
use proxy::TcpProxy;

const STEP_TIMEOUT: Duration = Duration::from_secs(45);
const ITERATION_TIMEOUT: Duration = Duration::from_secs(240);
const MAX_ITERATIONS: usize = 50;
const PEER_COUNT: usize = 3;
const RESTART_PEER: usize = PEER_COUNT - 1;

struct Peer {
    path: PathBuf,
    client: DbgClient,
}

pub(super) async fn run(opts: &TestOpts, backend: &BackendOpts) -> Result<()> {
    if opts.iterations == 0 || opts.iterations > MAX_ITERATIONS {
        bail!("durable-streams needs 1 to {MAX_ITERATIONS} iterations");
    }
    let local_backend = match backend.url.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    if !local_backend {
        bail!("durable-streams requires a local backend URL");
    }
    let parent = opts
        .state_directory
        .clone()
        .unwrap_or(App::data_directory()?.join("durable-streams"));
    std::fs::create_dir_all(&parent)?;
    let directory = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(parent)?
        .keep();
    info!(directory = %directory.display(), iterations = opts.iterations,
        "durable-streams keeps its peer databases in this directory");

    for iteration in 0..opts.iterations {
        let iteration_dir = directory.join(format!("iteration-{iteration}"));
        std::fs::create_dir(&iteration_dir)?;
        tokio::time::timeout(
            ITERATION_TIMEOUT,
            run_iteration(iteration, iteration_dir, backend),
        )
        .await
        .wrap_err_with(|| {
            format!(
                "durable-streams iteration exceeded {} seconds",
                ITERATION_TIMEOUT.as_secs()
            )
        })??;
        info!(
            iteration = iteration + 1,
            "durable-streams iteration passed"
        );
    }
    info!(iterations = opts.iterations, directory = %directory.display(),
        "durable-streams passed all convergence and decryption checks");
    Ok(())
}

async fn run_iteration(iteration: usize, directory: PathBuf, backend: &BackendOpts) -> Result<()> {
    let proxy = TcpProxy::start(backend).await?;
    let mut peers = Vec::with_capacity(PEER_COUNT);
    for index in 0..PEER_COUNT {
        let wallet = generate_wallet();
        let path = directory.join(format!("peer-{index}.db3"));
        let network = if index == RESTART_PEER {
            proxy.backend()
        } else {
            backend
        };
        let client = clients::new_disk_client_for(&wallet, path.clone(), network).await?;
        clients::register_client(&client, wallet.into_alloy()).await?;
        peers.push(Peer { path, client });
    }
    let group_id = {
        let group = peers[0].client.create_group(None, None)?;
        let inboxes = peers[1..]
            .iter()
            .map(|peer| peer.client.inbox_id())
            .collect::<Vec<_>>();
        group.add_members(&inboxes).await?;
        group.group_id
    };
    for peer in &peers[1..] {
        peer.client.sync_welcomes().await?;
    }
    converge(&peers, group_id).await?;

    info!(iteration, %group_id, phase = "commit-race", "checking durable streams");
    // Both public operations start from the same settled epoch.
    {
        let first = peers[0].client.group(&group_id)?;
        let second = peers[1].client.group(&group_id)?;
        let (name, description) = tokio::join!(
            first.update_group_name(format!("durable race {iteration}")),
            second.update_group_description(format!("concurrent change {iteration}")),
        );
        name.wrap_err("concurrent group-name commit failed")?;
        description.wrap_err("concurrent description commit failed")?;
    }
    converge(&peers, group_id).await?;
    for peer in &peers {
        let group = peer.client.group(&group_id)?;
        if group.group_name()? != format!("durable race {iteration}")
            || group.group_description()? != format!("concurrent change {iteration}")
        {
            bail!("concurrent commits lost one of the metadata changes");
        }
    }
    fresh_messages(&peers, group_id, iteration, "commit-race").await?;

    info!(iteration, %group_id, phase = "invalid-envelope", "checking durable streams");
    invalid_supported_envelope(&peers, group_id).await?;
    fresh_messages(&peers, group_id, iteration, "invalid-envelope").await?;

    info!(iteration, %group_id, phase = "stream-reconnect", "checking durable streams");
    // Dropping the stream releases its transport lease. The next stream catches up.
    let mut stream =
        StreamGroupMessages::new(&peers[RESTART_PEER].client.context, vec![group_id]).await?;
    let first = format!("durable/{iteration}/before-disconnect");
    peers[0]
        .client
        .group(&group_id)?
        .send_message(first.as_bytes(), Default::default())
        .await?;
    expect_stream_message(&mut stream, first.as_bytes()).await?;
    drop(stream);
    let gap = format!("durable/{iteration}/while-disconnected");
    peers[0]
        .client
        .group(&group_id)?
        .send_message(gap.as_bytes(), Default::default())
        .await?;
    let mut stream =
        StreamGroupMessages::new(&peers[RESTART_PEER].client.context, vec![group_id]).await?;
    expect_stream_message(&mut stream, gap.as_bytes()).await?;

    info!(iteration, %group_id, phase = "tcp-outage", "checking durable streams");
    network_outage(&peers, group_id, iteration, &proxy, &mut stream).await?;
    drop(stream);
    converge(&peers, group_id).await?;
    fresh_messages(&peers, group_id, iteration, "tcp-outage").await?;

    info!(iteration, %group_id, phase = "clean-reopen", "checking durable streams");
    // Reopen the same file. Do not copy state or create another installation.
    let peer = peers.pop().ok_or_else(|| eyre!("third peer is missing"))?;
    let installation = peer.client.installation_public_key().to_vec();
    peer.client.close().await?;
    let path = peer.path;
    drop(peer.client);
    let offline = format!("durable/{iteration}/while-closed");
    peers[0]
        .client
        .group(&group_id)?
        .send_message(offline.as_bytes(), Default::default())
        .await?;
    let client = clients::existing_client_inner_for(path.clone(), proxy.backend())?;
    if client.installation_public_key().to_vec() != installation {
        bail!("restart changed the persisted installation");
    }
    peers.push(Peer { path, client });
    converge(&peers, group_id).await?;
    expect_stored_message(
        &peers[RESTART_PEER].client.group(&group_id)?,
        offline.as_bytes(),
    )?;
    fresh_messages(&peers, group_id, iteration, "clean-reopen").await?;
    for peer in peers {
        peer.client.close().await?;
    }
    Ok(())
}

async fn network_outage(
    peers: &[Peer],
    group_id: GroupId,
    iteration: usize,
    proxy: &TcpProxy,
    stream: &mut StreamGroupMessages,
) -> Result<()> {
    let mut step = "pause-proxy";
    let result = tokio::time::timeout(STEP_TIMEOUT, async {
        let refused = proxy.pause().await?;
        info!(iteration, refused, step, "TCP outage step completed");
        let group = peers[0].client.group(&group_id)?;
        let image_url = format!("https://example.invalid/durable-outage-{iteration}.png");
        step = "connected-peer-commit";
        group
            .update_group_image_url_square(image_url.clone())
            .await?;
        info!(iteration, step, "TCP outage step completed");
        let gap = format!("durable/{iteration}/during-tcp-outage");
        step = "connected-peer-message";
        group
            .send_message(gap.as_bytes(), Default::default())
            .await?;
        info!(iteration, step, "TCP outage step completed");
        // The two connected peers must continue while the third peer is offline.
        step = "connected-peer-convergence";
        converge(&peers[..RESTART_PEER], group_id).await?;
        info!(iteration, step, "TCP outage step completed");
        step = "wait-for-refused-reconnect";
        let refusal = proxy.wait_for_refusal_after(refused);
        tokio::pin!(refusal);
        loop {
            tokio::select! {
                result = &mut refusal => {
                    result?;
                    break;
                }
                item = stream.next() => match item {
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => bail!("message stream closed during the TCP outage"),
                }
            }
        }
        info!(iteration, step, "TCP outage step completed");
        step = "resume-proxy";
        proxy.resume().await?;
        info!(iteration, step, "TCP outage step completed");
        // Reuse the existing reader. A transport reconnect must not close it.
        step = "same-reader-delivery";
        expect_stream_message(stream, gap.as_bytes()).await?;
        info!(iteration, step, "TCP outage step completed");
        step = "all-peer-convergence";
        converge(peers, group_id).await?;
        info!(iteration, step, "TCP outage step completed");
        for peer in peers {
            if peer.client.group(&group_id)?.group_image_url_square()? != image_url {
                bail!("peer missed the commit made during its TCP outage");
            }
            expect_stored_message(&peer.client.group(&group_id)?, gap.as_bytes())?;
        }
        Ok(())
    })
    .await;
    result.wrap_err_with(|| format!("TCP outage deadline expired during {step}"))?
}

async fn converge(peers: &[Peer], group_id: GroupId) -> Result<()> {
    tokio::time::timeout(
        STEP_TIMEOUT,
        try_join_all(
            peers
                .iter()
                .map(|peer| async move { peer.client.group(&group_id)?.sync().await }),
        ),
    )
    .await
    .wrap_err("group convergence timed out")??;
    let states = peers
        .iter()
        .map(|peer| current_state(&peer.client, group_id))
        .collect::<Result<Vec<_>>>()?;
    if states.windows(2).any(|pair| pair[0] != pair[1]) {
        let epochs = states.iter().map(|(epoch, _)| *epoch).collect::<Vec<_>>();
        bail!("peer epochs or epoch authenticators differ: epochs={epochs:?}");
    }
    Ok(())
}

fn current_state(client: &DbgClient, group_id: GroupId) -> Result<(u64, Vec<u8>)> {
    // The snapshot loads only after the database writer starts and does not escape it.
    Ok(client
        .context
        .mls_storage()
        .transaction(|tx| {
            let storage = tx.key_store();
            let group = OpenMlsGroup::load(&storage, &group_id.to_openmls())?
                .ok_or_else(|| StorageError::from(NotFound::MlsGroup(group_id)))?;
            Ok::<_, StorageError>(Continue((
                group.epoch().as_u64(),
                group.epoch_authenticator().as_slice().to_vec(),
            )))
        })?
        .into_continued())
}

async fn fresh_messages(
    peers: &[Peer],
    group_id: GroupId,
    iteration: usize,
    phase: &str,
) -> Result<()> {
    for (sender, peer) in peers.iter().enumerate() {
        let text = format!("durable/{iteration}/{phase}/{sender}");
        peer.client
            .group(&group_id)?
            .send_message(text.as_bytes(), Default::default())
            .await?;
        converge(peers, group_id).await?;
        for receiver in peers {
            expect_stored_message(&receiver.client.group(&group_id)?, text.as_bytes())?;
        }
    }
    Ok(())
}

fn expect_stored_message(group: &MlsGroup<MlsContext>, expected: &[u8]) -> Result<()> {
    let occurrences = group
        .find_messages(&MsgQueryArgs::default())?
        .iter()
        .filter(|message| message.decrypted_message_bytes == expected)
        .count();
    if occurrences != 1 {
        bail!("fresh message has {occurrences} stored copies; expected one");
    }
    Ok(())
}

async fn expect_stream_message(stream: &mut StreamGroupMessages, expected: &[u8]) -> Result<()> {
    tokio::time::timeout(STEP_TIMEOUT, async {
        while let Some(message) = stream.next().await {
            if message?.decrypted_message_bytes == expected {
                return Ok(());
            }
        }
        bail!("message stream ended before the expected message");
    })
    .await
    .wrap_err("message stream did not recover before its deadline")?
}

async fn invalid_supported_envelope(peers: &[Peer], group_id: GroupId) -> Result<()> {
    let before = peers
        .iter()
        .map(|peer| current_state(&peer.client, group_id))
        .collect::<Result<Vec<_>>>()?;
    let topic = Topic::new_group_message(group_id);
    let mut rows = peers[0]
        .client
        .context
        .api()
        .query_all(
            [(topic.clone(), Cursor(0))].into(),
            xmtp_configuration::BACKEND_DEFAULT_MAX_QUERY_LIMIT as u32,
        )
        .await?;
    let mut envelope = rows
        .pop()
        .and_then(|row| row.envelope)
        .ok_or_else(|| eyre!("group has no envelope to corrupt"))?;
    let Some(Payload::GroupMessage(message)) = &mut envelope.payload else {
        bail!("last group envelope has the wrong payload type");
    };
    if message.data.get(..2) != Some(&[0, 1]) {
        bail!("last envelope does not use the supported MLS version");
    }
    // Keep the supported TLS structure. Change one byte of authenticated ciphertext.
    *message
        .data
        .last_mut()
        .ok_or_else(|| eyre!("empty MLS ciphertext"))? ^= 1;
    let receipts = peers[0]
        .client
        .context
        .api()
        .publish_units(vec![PublishUnit::single(envelope)?])
        .await?;
    let receipt = receipts
        .last()
        .ok_or_else(|| eyre!("invalid input has no publish receipt"))?;
    let (_, cursor, _) = xmtp_api_backend::envelope::metadata(receipt, topic.kind())?;
    for (index, peer) in peers.iter().enumerate() {
        wait_through(
            &peer.client.context,
            [(topic.clone(), cursor)].into(),
            Some(STEP_TIMEOUT),
        )
        .await?;
        let rejection = peer
            .client
            .db()
            .read_last_rejection(&StreamTopic::group(group_id))?
            .ok_or_else(|| eyre!("peer {index} has no terminal rejection"))?;
        if rejection.sequence_id != cursor {
            bail!("peer {index} did not reject the invalid envelope at its received cursor");
        }
        if current_state(&peer.client, group_id)? != before[index] {
            bail!("peer {index} changed its MLS epoch after invalid input");
        }
    }
    Ok(())
}

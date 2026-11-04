//! Native processes share the production encrypted database, not test tables.

use crate::groups::test_support::{
    EnginePeer, EngineSuiteOps, EngineTestGroup, EngineTestPeer, EngineTestPeerOps,
};
use crate::prelude::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::Mutex,
};

use openmls::group::MlsGroup as OpenMlsGroup;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use xmtp_common::wait_for_some;
use xmtp_db::{
    StorageOption, TransactionalKeyStore, XmtpMlsStorageProvider,
    incoming_envelope::{QueryIncomingEnvelope, StreamTopic},
};
use xmtp_proto::types::{Cursor, GroupId, Topic};

use crate::{
    context::XmtpSharedContext,
    core::precommit_test_hook,
    groups::{send_message_opts::SendMessageOpts, test_support::MlsGroupExt},
};
use alloy::signers::local::PrivateKeySigner;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

const CHILD_CONFIG: &str = "XMTP_STATE_PROCESS_TEST_CONFIG";
const CHILD_TEST: &str = "groups::tests::test_state_processes::state_process_child";

#[derive(Serialize, Deserialize)]
enum ChildMode {
    Receive,
    CrashBeforeCommit { target: u64, epoch: u64 },
}

#[derive(Serialize, Deserialize)]
struct ChildConfig {
    database: String,
    owner: Vec<u8>,
    group_id: GroupId,
    mode: ChildMode,
    ready: PathBuf,
    start: PathBuf,
    report: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct StateReport {
    epoch: u64,
    authenticator: Vec<u8>,
    last_message: Option<Vec<u8>>,
    processed: u64,
    received: u64,
    pending: usize,
}

struct StateProcess {
    child: Mutex<Child>,
    ready: PathBuf,
    report: PathBuf,
    log: PathBuf,
}

impl StateProcess {
    fn spawn(
        directory: &Path,
        name: &str,
        database: &str,
        owner: &[u8],
        group_id: GroupId,
        mode: ChildMode,
        start: &Path,
    ) -> TestResult<Self> {
        let ready = directory.join(format!("{name}.ready.json"));
        let report = directory.join(format!("{name}.report.json"));
        let log = directory.join(format!("{name}.log"));
        let config = ChildConfig {
            database: database.into(),
            owner: owner.into(),
            group_id,
            mode,
            ready: ready.clone(),
            start: start.into(),
            report: report.clone(),
        };
        let stdout = fs::File::create(&log)?;
        let child = Command::new(std::env::current_exe()?)
            .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD_CONFIG, serde_json::to_string(&config)?)
            .stdout(Stdio::from(stdout.try_clone()?))
            .stderr(Stdio::from(stdout))
            .spawn()?;
        Ok(Self {
            child: Mutex::new(child),
            ready,
            report,
            log,
        })
    }

    fn diagnostics(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    async fn read_report(&self, path: &Path) -> TestResult<StateReport> {
        let report = wait_for_some(|| async {
            if let Ok(bytes) = fs::read(path)
                && let Ok(report) = serde_json::from_slice::<StateReport>(&bytes)
            {
                return Some(Ok(report));
            }
            self.child
                .lock()
                .unwrap()
                .try_wait()
                .unwrap()
                .map(|status| Err(format!("child exited before its report: {status}")))
        })
        .await;
        match report {
            Some(Ok(report)) => Ok(report),
            other => Err(format!("child report failed: {other:?}\n{}", self.diagnostics()).into()),
        }
    }

    async fn finish(&self) -> TestResult<()> {
        let status = wait_for_some(|| async { self.child.lock().unwrap().try_wait().unwrap() })
            .await
            .ok_or_else(|| format!("child did not exit\n{}", self.diagnostics()))?;
        if !status.success() {
            return Err(format!("child failed: {status}\n{}", self.diagnostics()).into());
        }
        Ok(())
    }

    fn kill(&self) -> TestResult<ExitStatus> {
        let mut child = self.child.lock().unwrap();
        child.kill()?;
        Ok(child.wait()?)
    }
}

impl Drop for StateProcess {
    fn drop(&mut self) {
        let child = self
            .child
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();
    }
}

// A persistent Tester creates the identity. Each child opens that same identity.
async fn reopen(owner: &[u8], database: &str) -> TestResult<EngineTestPeer> {
    let owner = PrivateKeySigner::from_slice(owner)?;
    Ok(EngineTestPeer::open_persistent(
        owner,
        database,
        crate::identity_lifecycle::IdentityStrategy::CachedOnly,
        |_| {},
    )
    .await)
}

async fn snapshot(group: &EngineTestGroup) -> TestResult<StateReport> {
    let topic = StreamTopic::group(group.group_id);
    let db = group.context.db();
    let progress = db.topic_progress(&topic)?;
    Ok(StateReport {
        epoch: group.epoch().await?,
        authenticator: group.epoch_authenticator().await?,
        last_message: group
            .find_messages(&Default::default())?
            .last()
            .map(|message| message.decrypted_message_bytes.clone()),
        processed: progress.processed.0,
        received: progress.received.0,
        pending: db.pending_states_through(&topic, progress.received)?.len(),
    })
}

async fn wait_until_idle(client: &EngineTestPeer) {
    assert!(
        wait_for_some(|| async {
            client
                .context
                .incoming_runtime()
                .coordinator
                .lock()
                .is_none()
                .then_some(())
        })
        .await
        .is_some(),
        "the setup receiver did not stop"
    );
}

fn database_path(client: &EngineTestPeer) -> String {
    match client.context.store().opts() {
        StorageOption::Persistent(path) => path.clone(),
        StorageOption::Ephemeral => panic!("process tests need a persistent database"),
    }
}

/// Supporting entry point. Only an exact child invocation sets its configuration.
#[xmtp_common::test(unwrap_try = true)]
async fn state_process_child() {
    let Ok(config) = std::env::var(CHILD_CONFIG) else {
        return;
    };
    let config: ChildConfig = serde_json::from_str(&config)?;
    let client = reopen(&config.owner, &config.database).await?;
    let group = client.group(&config.group_id)?;
    fs::write(&config.ready, serde_json::to_vec(&snapshot(&group).await?)?)?;
    assert!(
        wait_for_some(|| async { config.start.is_file().then_some(()) })
            .await
            .is_some(),
        "parent did not release the start gate"
    );
    match config.mode {
        ChildMode::Receive => {
            group.receive().await?;
            fs::write(
                &config.report,
                serde_json::to_vec(&snapshot(&group).await?)?,
            )?;
        }
        ChildMode::CrashBeforeCommit { target, epoch } => {
            let group_id = config.group_id;
            let _hook = precommit_test_hook::install(move |conn| {
                let storage = conn.key_store();
                let topic = StreamTopic::group(group_id);
                let progress = storage.db().topic_progress(&topic).unwrap();
                if progress.processed != Cursor(target) {
                    return;
                }
                let mls = OpenMlsGroup::load(&storage, &group_id.to_openmls())
                    .unwrap()
                    .unwrap();
                assert_eq!(mls.epoch().as_u64(), epoch);
                let pending = storage
                    .db()
                    .pending_states_through(&topic, progress.received)
                    .unwrap();
                assert!(pending.is_empty());
                let report = StateReport {
                    epoch,
                    authenticator: mls.epoch_authenticator().as_slice().to_vec(),
                    last_message: None,
                    processed: progress.processed.0,
                    received: progress.received.0,
                    pending: pending.len(),
                };
                fs::write(&config.report, serde_json::to_vec(&report).unwrap()).unwrap();
                // Keep the writer open until the parent kills this process.
                let (_sender, receiver) = std::sync::mpsc::channel::<()>();
                receiver.recv().unwrap();
            });
            group.process_pending_group_head(None)?;
            panic!("the ordered commit did not reach the precommit hook");
        }
    }
}

/// Independent receivers retain ordered MLS state in one database.
#[xmtp_common::test(unwrap_try = true)]
async fn independent_processes_apply_ordered_commits_to_one_database() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::persistent().await;
    let peer = alix.create_group(None, None)?;
    peer.invite(&bo).await?;
    let shared = bo.wait_for_welcomes().await?.pop()?;
    shared.receive().await?;
    wait_until_idle(&bo).await;
    let initial = snapshot(&shared).await?;
    let database = database_path(&bo);
    // zeroize: none, the child process reads this test key from its config file
    #[allow(clippy::disallowed_methods)]
    let owner = bo.owner.to_bytes().to_vec();
    let group_id = shared.group_id;
    for name in [
        "first ordered commit",
        "second ordered commit",
        "third ordered commit",
    ] {
        peer.update_group_name(name.into()).await?;
    }
    peer.send_message(b"after ordered commits", SendMessageOpts::default())
        .await?;
    let expected = snapshot(&peer).await?;
    assert_eq!(expected.epoch, initial.epoch + 3);

    let directory = TempDir::new()?;
    let start = directory.path().join("start");
    let first = StateProcess::spawn(
        directory.path(),
        "first",
        &database,
        &owner,
        group_id,
        ChildMode::Receive,
        &start,
    )?;
    let second = StateProcess::spawn(
        directory.path(),
        "second",
        &database,
        &owner,
        group_id,
        ChildMode::Receive,
        &start,
    )?;
    for child in [&first, &second] {
        let ready = child.read_report(&child.ready).await?;
        assert_eq!(ready.epoch, initial.epoch);
        assert_eq!(ready.authenticator, initial.authenticator);
    }
    fs::write(&start, b"start")?;
    for child in [&first, &second] {
        let report = child.read_report(&child.report).await?;
        child.finish().await?;
        assert_eq!(report.epoch, expected.epoch);
        assert_eq!(report.authenticator, expected.authenticator);
        assert_eq!(
            report.last_message.as_deref(),
            Some(b"after ordered commits".as_slice())
        );
        assert_eq!(report.processed, report.received);
        assert_eq!(report.pending, 0);
    }

    let reopened = reopen(&owner, &database).await?;
    let recovered = reopened.group(&group_id)?;
    recovered
        .send_message(b"after both processes", SendMessageOpts::default())
        .await?;
    peer.receive().await?;
    assert_eq!(
        peer.test_last_message_bytes().await?,
        Some(b"after both processes".to_vec())
    );
    assert_eq!(
        recovered.epoch_authenticator().await?,
        peer.epoch_authenticator().await?
    );
}

/// Process death cannot commit part of an incoming MLS state change.
#[xmtp_common::test(unwrap_try = true)]
async fn process_death_before_state_commit_preserves_replay_and_convergence() {
    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::persistent().await;
    let peer = alix.create_group(None, None)?;
    peer.invite(&bo).await?;
    let shared = bo.wait_for_welcomes().await?.pop()?;
    shared.receive().await?;
    wait_until_idle(&bo).await;
    let before = snapshot(&shared).await?;
    let database = database_path(&bo);
    // zeroize: none, the child process reads this test key from its config file
    #[allow(clippy::disallowed_methods)]
    let owner = bo.owner.to_bytes().to_vec();
    let group_id = shared.group_id;
    let topic = StreamTopic::group(group_id);
    peer.update_group_name("commit interrupted before persistence".into())
        .await?;
    let expected = snapshot(&peer).await?;
    assert_eq!(expected.epoch, before.epoch + 1);
    bo.mls_store()
        .receive_topics_once(
            &[Topic::new_group_message(group_id)],
            bo.context
                .incoming_runtime()
                .policy()
                .incoming_limits(topic.kind),
        )
        .await?;
    let admitted = snapshot(&shared).await?;
    assert_eq!(admitted.epoch, before.epoch);
    assert_eq!(admitted.processed, before.processed);
    // The metadata update has an AppDataUpdate proposal and its commit.
    assert_eq!(admitted.pending, 2);
    // Dictionary-native metadata updates use a proposal by reference. Apply
    // its ordered prefix before the child attempts the referenced commit.
    shared.process_pending_group_head(None)?;
    let before_commit = snapshot(&shared).await?;
    assert_eq!(before_commit.epoch, before.epoch);
    assert_eq!(before_commit.pending, 1);
    let crypto_before = bo.context.mls_storage().hash_all()?;
    let pending = bo.context.db().first_pending_envelope(&topic)??;
    let directory = TempDir::new()?;
    let start = directory.path().join("start");
    let child = StateProcess::spawn(
        directory.path(),
        "crash",
        &database,
        &owner,
        group_id,
        ChildMode::CrashBeforeCommit {
            target: admitted.received,
            epoch: expected.epoch,
        },
        &start,
    )?;
    let ready = child.read_report(&child.ready).await?;
    assert_eq!(ready.epoch, before_commit.epoch);
    fs::write(&start, b"start")?;
    let uncommitted = child.read_report(&child.report).await?;
    assert_eq!(uncommitted.epoch, expected.epoch);
    assert_eq!(uncommitted.authenticator, expected.authenticator);
    assert_eq!(uncommitted.processed, admitted.received);
    assert_eq!(uncommitted.pending, 0);
    assert!(!child.kill()?.success());

    let reopened = reopen(&owner, &database).await?;
    let recovered = reopened.group(&group_id)?;
    let rolled_back = snapshot(&recovered).await?;
    assert_eq!(rolled_back.epoch, before_commit.epoch);
    assert_eq!(rolled_back.authenticator, before_commit.authenticator);
    assert_eq!(rolled_back.processed, before_commit.processed);
    assert_eq!(rolled_back.received, before_commit.received);
    assert_eq!(rolled_back.pending, before_commit.pending);
    assert_eq!(reopened.context.mls_storage().hash_all()?, crypto_before);
    let replay = reopened.context.db().first_pending_envelope(&topic)??;
    assert_eq!(replay.sequence_id, pending.sequence_id);
    assert_eq!(replay.envelope, pending.envelope);
    recovered.receive().await?;
    let completed = snapshot(&recovered).await?;
    assert_eq!(completed.epoch, expected.epoch);
    assert_eq!(completed.authenticator, expected.authenticator);
    assert_eq!(completed.processed, admitted.received);
    assert_eq!(completed.pending, 0);
    peer.send_message(b"after process recovery", SendMessageOpts::default())
        .await?;
    recovered.receive().await?;
    assert_eq!(
        recovered.test_last_message_bytes().await?,
        Some(b"after process recovery".to_vec())
    );
    recovered
        .send_message(b"recovery confirmed", SendMessageOpts::default())
        .await?;
    peer.receive().await?;
    assert_eq!(
        peer.test_last_message_bytes().await?,
        Some(b"recovery confirmed".to_vec())
    );
}

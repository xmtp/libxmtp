//! Browser attachment tests that need the origin private file system.
//!
//! OPFS sync access handles exist only in a dedicated worker. The runner
//! setting below applies to the whole test binary, so these tests live in their
//! own binary. The `xmtp_mls` unit tests keep the default runner.
#![cfg(target_arch = "wasm32")]
#![recursion_limit = "256"]

use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use xmtp_attachments::{
    AttachmentFailureCause as Cause, LocalStore, OpfsStore, plaintext_rel_path, staged_path,
};
use xmtp_common::time::Duration;
use xmtp_configuration::{AttachmentsConfiguration, ServerConfiguration, StaticConfigProvider};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::{
    ConnectionExt,
    diesel::RunQueryDsl,
    prelude::{QueryLocalAttachment, QueryPendingAttachment},
};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_mls::{
    Client,
    attachments::{
        AttachmentSource, PendingAttachmentStatus, pause_next_create_move,
        pause_next_download_move, set_next_download_body,
    },
    storage_location::{
        DeploymentWriteFault, record_deployment_for_test, set_deployment_write_fault,
        write_deployments_for_test,
    },
    utils::test::identity_setup,
};
use xmtp_proto::backend_v1::{GetInboxIdsResponse, get_inbox_ids_response};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

fn offline_api() -> xmtp_api_backend::MockBackendClient {
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_get_inbox_ids().times(1).returning(|request| {
        Ok(GetInboxIdsResponse {
            responses: request
                .requests
                .into_iter()
                .map(|request| get_inbox_ids_response::Response {
                    identifier: request.identifier,
                    identifier_kind: request.identifier_kind,
                    inbox_id: None,
                })
                .collect(),
        })
    });
    api
}

async fn opfs_client(
    api: xmtp_api_backend::MockBackendClient,
    root: String,
) -> Client<impl xmtp_mls::context::XmtpSharedContext> {
    Client::builder(identity_setup(generate_local_wallet()))
        .api_client(api)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .temp_store()
        .await
        .default_mls_store()
        .expect("MLS store setup failed")
        .config_provider(Arc::new(StaticConfigProvider::edited(
            |configuration: &mut ServerConfiguration| {
                configuration.attachments = Some(AttachmentsConfiguration {
                    base_url: "https://example.com/attachments".into(),
                    max_upload_bytes: 1_048_576,
                    retention_seconds: 0,
                });
            },
        )))
        .attachments_dir(root)
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await
        .expect("client build failed")
}

fn test_root(name: &str) -> String {
    format!("{name}/{}", hex::encode(xmtp_common::rand_array::<16>()))
}

async fn assert_deployment_write_fault(
    fault: DeploymentWriteFault,
) -> Result<(), xmtp_common::BoxDynError> {
    let root = test_root("deployment-write-fault-tests");
    write_deployments_for_test(Path::new(&root), b"prior record").await?;
    set_deployment_write_fault(fault);
    assert!(
        write_deployments_for_test(Path::new(&root), b"new record")
            .await
            .is_err()
    );
    let store = OpfsStore::new(&root).await?;
    assert_eq!(
        store
            .open_read("deployments.json")
            .await?
            .read_chunk(0, 64)
            .await?,
        b"prior record"
    );
    assert!(
        !store
            .list_files()
            .await?
            .iter()
            .any(|file| file.path.starts_with(".tmp/deployments-"))
    );
    Ok(())
}

// verifies: ATCH-040, ATCH-069
#[xmtp_common::test(unwrap_try = true)]
async fn deployment_temp_removed_after_write_error() {
    assert_deployment_write_fault(DeploymentWriteFault::Write).await?;
}

// verifies: ATCH-040, ATCH-069
#[xmtp_common::test(unwrap_try = true)]
async fn deployment_temp_removed_after_sync_error() {
    assert_deployment_write_fault(DeploymentWriteFault::Sync).await?;
}

// verifies: ATCH-040, ATCH-069
#[xmtp_common::test(unwrap_try = true)]
async fn deployment_temp_removed_after_replace_error() {
    assert_deployment_write_fault(DeploymentWriteFault::Replace).await?;
}

// verifies: ATCH-040, ATCH-069
#[xmtp_common::test(unwrap_try = true)]
async fn deployment_temp_removed_after_cancel() {
    let root = test_root("deployment-write-cancel-tests");
    write_deployments_for_test(Path::new(&root), b"prior record").await?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    set_deployment_write_fault(DeploymentWriteFault::Pause {
        entered: entered.clone(),
        resume,
    });
    let path = std::path::PathBuf::from(&root);
    let (write, abort) = futures::future::abortable(async move {
        write_deployments_for_test(&path, b"new record").await
    });
    let task = xmtp_common::task::spawn(write);
    xmtp_common::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    abort.abort();
    assert!(task.await?.is_err());
    let store = OpfsStore::new(&root).await?;
    xmtp_common::time::timeout(Duration::from_secs(3), async {
        while store
            .list_files()
            .await
            .expect("list deployment temp files")
            .iter()
            .any(|file| file.path.starts_with(".tmp/deployments-"))
        {
            xmtp_common::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        store
            .open_read("deployments.json")
            .await?
            .read_chunk(0, 64)
            .await?,
        b"prior record"
    );
}

// verifies: ATCH-069
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_record_keeps_both_backend_urls() {
    let root = test_root("deployment-record-cancel-race");
    let path = std::path::PathBuf::from(&root);
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    let settled = Arc::new(tokio::sync::Notify::new());
    set_deployment_write_fault(DeploymentWriteFault::PauseReplace {
        entered: entered.clone(),
        resume: resume.clone(),
        settled: settled.clone(),
    });
    let first_path = path.clone();
    let (first, abort) = futures::future::abortable(async move {
        record_deployment_for_test(&first_path, "https://first.example", "first").await
    });
    let first_task = xmtp_common::task::spawn(first);
    xmtp_common::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    abort.abort();
    assert!(first_task.await?.is_err());

    let (sent, mut received) = tokio::sync::oneshot::channel();
    drop(xmtp_common::task::spawn(async move {
        let _ =
            sent.send(record_deployment_for_test(&path, "https://second.example", "second").await);
    }));
    let early = xmtp_common::time::timeout(Duration::from_millis(500), &mut received).await;
    resume.notify_one();
    match early {
        Ok(result) => result??,
        Err(_) => xmtp_common::time::timeout(Duration::from_secs(3), received).await???,
    }
    xmtp_common::time::timeout(Duration::from_secs(3), settled.notified()).await?;
    let store = OpfsStore::new(&root).await?;
    let file = store.open_read("deployments.json").await?;
    let bytes = file.read_chunk(0, 64 * 1024).await?;
    let document: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(document["deployments"]["https://first.example"], "first");
    assert_eq!(document["deployments"]["https://second.example"], "second");
}

// verifies: ATCH-048
#[xmtp_common::test(unwrap_try = true)]
async fn client_create_writes_plaintext_to_opfs() {
    let root = test_root("attachment-client-tests");
    let client = opfs_client(offline_api(), root.clone()).await;
    let pending = client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"opfs plaintext".to_vec(),
            filename: Some("proof.txt".into()),
            mime_type: "text/plain".into(),
        })
        .await?;
    let remote = pending.remote_attachment();
    let local = plaintext_rel_path(remote)?;
    let staged = staged_path(&remote.content_digest)?;
    let root_store = OpfsStore::new_root().await?;
    let local_file = root_store.open_read(&format!("{root}/{local}")).await?;
    assert_eq!(local_file.read_chunk(0, 64).await?, b"opfs plaintext");
    assert!(root_store.exists(&format!("{root}/{staged}")).await?);
}

// verifies: ATCH-046, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_create_waits_for_published_opfs_move() {
    let root = test_root("attachment-move-cancel-tests");
    let client = opfs_client(offline_api(), root.clone()).await;
    let store = OpfsStore::new(&root).await?;
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    pause_next_create_move(entered.clone(), resume.clone());
    let source = AttachmentSource::Bytes {
        bytes: b"cancel during OPFS move".to_vec(),
        filename: Some("proof.txt".into()),
        mime_type: "text/plain".into(),
    };
    let creating_client = client.clone();
    let (create, abort) =
        futures::future::abortable(
            async move { creating_client.attachments().create(source).await },
        );
    let task = xmtp_common::task::spawn(create);
    xmtp_common::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    let local = store
        .list_files()
        .await?
        .into_iter()
        .map(|file| file.path)
        .find(|path| !path.starts_with(".tmp/") && !path.starts_with(".staged/"))
        .expect("plaintext move took effect");
    let key = local.split('/').next().expect("attachment key").to_owned();
    abort.abort();
    assert!(task.await?.is_err());
    resume.notify_one();
    xmtp_common::time::timeout(Duration::from_secs(3), async {
        while !matches!(store.list_files().await, Ok(files) if files.is_empty()) {
            xmtp_common::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert!(!store.exists(&key).await?);
    assert!(client.db().list_local_attachments()?.is_empty());
    assert!(client.db().list_pending_attachments_since(0)?.is_empty());
}

// verifies: ATCH-047, ATCH-062, ATCH-063
#[xmtp_common::test(unwrap_try = true)]
async fn cancelled_download_keeps_publication_locked_until_opfs_move_settles() {
    let root = test_root("attachment-download-move-cancel-tests");
    let client = opfs_client(offline_api(), root.clone()).await;
    let store = OpfsStore::new(&root).await?;
    let pending = client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"cancel download during OPFS move".to_vec(),
            filename: Some("proof.txt".into()),
            mime_type: "text/plain".into(),
        })
        .await?;
    let remote = pending.remote_attachment().clone();
    let local = plaintext_rel_path(&remote)?;
    let staged = staged_path(&remote.content_digest)?;
    let file = store.open_read(&staged).await?;
    let body = file.read_chunk(0, file.len() as usize).await?;
    client.db().delete_local_attachment(&local)?;
    store.remove_file(&local).await?;
    set_next_download_body(body);
    let entered = Arc::new(tokio::sync::Notify::new());
    let resume = Arc::new(tokio::sync::Notify::new());
    pause_next_download_move(entered.clone(), resume.clone());
    let downloading_client = client.clone();
    let downloading_remote = remote.clone();
    let download = xmtp_common::task::spawn(async move {
        downloading_client
            .attachments()
            .download(&downloading_remote)
            .await
    });
    xmtp_common::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    let deleting_client = client.clone();
    let deleting_remote = remote.clone();
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel();
    drop(xmtp_common::task::spawn(async move {
        let _ = done_tx.send(
            deleting_client
                .attachments()
                .delete_local(&deleting_remote)
                .await,
        );
    }));
    xmtp_common::time::sleep(Duration::from_millis(100)).await;
    assert!(
        done_rx.try_recv().is_err(),
        "deletion passed the in-flight move"
    );
    resume.notify_one();
    xmtp_common::time::timeout(Duration::from_secs(3), done_rx).await???;
    let download_result = xmtp_common::time::timeout(Duration::from_secs(3), download).await??;
    assert!(matches!(download_result, Err(error) if error.cause == Cause::Deleted));
    assert!(!store.exists(&local).await?);
    assert!(client.db().get_local_attachment(&local)?.is_none());
}

// verifies: ATCH-052, ATCH-062
#[xmtp_common::test(unwrap_try = true)]
async fn directory_at_plaintext_path_is_not_a_download() {
    let root = test_root("attachment-directory-download-tests");
    let client = opfs_client(offline_api(), root.clone()).await;
    let pending = client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"directory proof".to_vec(),
            filename: Some("proof.txt".into()),
            mime_type: "text/plain".into(),
        })
        .await?;
    let remote = pending.remote_attachment();
    let local = plaintext_rel_path(remote)?;
    client.db().delete_local_attachment(&local)?;
    let root_store = OpfsStore::new_root().await?;
    root_store.remove_file(&format!("{root}/{local}")).await?;
    root_store
        .create_dir_if_absent(&format!("{root}/{local}"))
        .await?;
    let result = client.attachments().download(remote).await;
    assert!(matches!(result, Err(error) if error.cause == Cause::LocalStorage));
    assert!(client.db().get_local_attachment(&local)?.is_none());
}

// verifies: ATCH-025, ATCH-074
#[xmtp_common::test(unwrap_try = true)]
async fn upload_failure_uses_wasm_timers() {
    let root = test_root("attachment-upload-timer-tests");
    let mut api = offline_api();
    api.expect_create_upload().times(1).returning(|_| {
        Err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(tonic::Status::invalid_argument(
                "upload rejected",
            )),
        ))
    });
    let client = opfs_client(api, root).await;
    let pending = client
        .attachments()
        .create(AttachmentSource::Bytes {
            bytes: b"timer proof".to_vec(),
            filename: None,
            mime_type: "text/plain".into(),
        })
        .await?;
    let digest = pending.remote_attachment().content_digest.clone();
    // A locked-database error is transient, so the client retries the outcome
    // write after a wasm timer until the trigger is gone.
    client.db().raw_query(|conn| {
        xmtp_db::diesel::sql_query(
            "CREATE TRIGGER lock_attachment_outcome BEFORE UPDATE OF status ON pending_attachments \
             WHEN NEW.status = 'failed' \
             BEGIN SELECT RAISE(ABORT, 'database table is locked'); END",
        )
        .execute(conn)
    })?;
    let unlocked = Arc::new(AtomicBool::new(false));
    let unlock = {
        let db = client.db();
        let unlocked = unlocked.clone();
        xmtp_common::task::spawn(async move {
            xmtp_common::time::sleep(Duration::from_millis(300)).await;
            db.raw_query(|conn| {
                xmtp_db::diesel::sql_query("DROP TRIGGER lock_attachment_outcome").execute(conn)
            })
            .expect("drop trigger");
            unlocked.store(true, Ordering::SeqCst);
        })
    };
    let error = xmtp_common::time::timeout(Duration::from_secs(5), pending.upload())
        .await?
        .unwrap_err();
    assert_eq!(error.cause, Cause::BackendRejected);
    assert!(unlocked.load(Ordering::SeqCst));
    unlock.await?;
    let row = client.db().get_pending_attachment(&digest)?.unwrap();
    assert_eq!(row.status, "failed");
    assert!(matches!(
        pending.status(),
        PendingAttachmentStatus::Failed(_)
    ));
}

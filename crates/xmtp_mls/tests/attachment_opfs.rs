//! Browser attachment tests that need the origin private file system.
//!
//! OPFS sync access handles exist only in a dedicated worker. The runner
//! setting below applies to the whole test binary, so these tests live in their
//! own binary. The `xmtp_mls` unit tests keep the default runner.
#![cfg(target_arch = "wasm32")]
#![recursion_limit = "256"]

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
use xmtp_db::{ConnectionExt, diesel::RunQueryDsl, prelude::QueryPendingAttachment};
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_mls::{
    Client,
    attachments::{AttachmentSource, PendingAttachmentStatus},
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

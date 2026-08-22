//! Tests for the read-only database integrity check: the client method
//! (`FfiXmtpClient::db_integrity_check`) and the free function
//! (`check_database_integrity`).

use crate::{DbOptions, check_database_integrity};
use xmtp_db::EncryptedMessageStore;

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_db_integrity_check_ok() {
    let client = new_test_client().await;
    let outcome = client.db_integrity_check(None).await.unwrap();
    assert_eq!(outcome.outcome, "ok");
    assert!(outcome.findings.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_check_database_integrity_free_fn_ok() {
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let ident = ffi_inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();
    let db_path = tmp_path();
    let key: Vec<u8> = EncryptedMessageStore::<()>::generate_enc_key().into();

    let client = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(db_path.clone()), Some(key.clone()), None, None, None),
        &inbox_id,
        ident,
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    register_client_with_wallet(&ffi_inbox_owner, &client).await;
    // Release the DB connection before checking the file out from under a
    // still-open client.
    client.shutdown().await.unwrap();

    let outcome = check_database_integrity(db_path, Some(key), None)
        .await
        .unwrap();
    assert_eq!(outcome.outcome, "ok");
    assert!(outcome.findings.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_check_database_integrity_free_fn_wrong_key_is_unreadable() {
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let ident = ffi_inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();
    let db_path = tmp_path();
    let key: Vec<u8> = EncryptedMessageStore::<()>::generate_enc_key().into();

    let client = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(db_path.clone()), Some(key), None, None, None),
        &inbox_id,
        ident,
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    register_client_with_wallet(&ffi_inbox_owner, &client).await;
    client.shutdown().await.unwrap();

    let wrong_key: Vec<u8> = EncryptedMessageStore::<()>::generate_enc_key().into();
    let outcome = check_database_integrity(db_path, Some(wrong_key), None)
        .await
        .unwrap();
    assert_eq!(outcome.outcome, "unreadable");
}

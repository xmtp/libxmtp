//! Tests for client creation, registration, identity management, and wallet operations

use crate::DbOptions;

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_create_client_with_storage() {
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let ident = ffi_inbox_owner.identifier();
    let nonce = 1;
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let path = tmp_path();

    let client_a = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(path.clone()), None, None, None, None),
        &inbox_id,
        ffi_inbox_owner.identifier(),
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
    register_client_with_wallet(&ffi_inbox_owner, &client_a).await;

    let installation_pub_key = client_a.inner_client.installation_public_key().to_vec();
    drop(client_a);

    let client_b = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(path.clone()), None, None, None, None),
        &inbox_id,
        ffi_inbox_owner.identifier(),
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

    let other_installation_pub_key = client_b.inner_client.installation_public_key().to_vec();
    drop(client_b);

    assert!(
        installation_pub_key == other_installation_pub_key,
        "did not use same installation ID"
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_create_client_with_key() {
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let nonce = 1;
    let ident = ffi_inbox_owner.identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let path = tmp_path();

    let key = static_enc_key().to_vec();

    let client_a = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(path.clone()), Some(key), None, None, None),
        &inbox_id,
        ffi_inbox_owner.identifier(),
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

    drop(client_a);

    let mut other_key = static_enc_key();
    other_key[31] = 1;

    let result_errored = create_client(
        connect_to_backend_test().await,
        DbOptions::new(
            Some(path.clone()),
            Some(other_key.to_vec()),
            None,
            None,
            None,
        ),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .is_err();

    assert!(result_errored, "did not error on wrong encryption key")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_can_message() {
    let amal = FfiWalletInboxOwner::new();
    let amal_ident = amal.identifier();
    let nonce = 1;
    let amal_inbox_id = amal_ident.inbox_id(nonce).unwrap();

    let bola = FfiWalletInboxOwner::new();
    let bola_ident = bola.identifier();
    let bola_inbox_id = bola_ident.inbox_id(nonce).unwrap();
    let path = tmp_path();

    let client_amal = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(path.clone()), None, None, None, None),
        &amal_inbox_id,
        amal.identifier(),
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

    // let coda = new_passkey_cred().await;
    // Check if can message a passkey identifier
    // TODO: enable when xmtp-node-go is updated
    // let can_msg = client_amal
    // .can_message(vec![coda.client.account_identifier.clone()])
    // .await
    // .unwrap();
    // let can_msg = *can_msg
    // .get(&coda.client.account_identifier)
    // .unwrap_or(&false);
    // assert!(can_msg);

    let can_message_result = client_amal
        .can_message(vec![bola.identifier()])
        .await
        .unwrap();

    assert!(
        can_message_result
            .get(&bola.identifier())
            .map(|&value| !value)
            .unwrap_or(false),
        "Expected the can_message result to be false for the address"
    );

    let client_bola = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(path.clone()), None, None, None, None),
        &bola_inbox_id,
        bola.identifier(),
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
    register_client_with_wallet(&bola, &client_bola).await;

    let can_message_result2 = client_amal
        .can_message(vec![bola.identifier()])
        .await
        .unwrap();

    assert!(
        can_message_result2
            .get(&bola.identifier())
            .copied()
            .unwrap_or(false),
        "Expected the can_message result to be true for the address"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_key_package_validation() {
    // Create a test client
    let client = Tester::new().await;

    // Get the client's inbox state to retrieve installation IDs
    let inbox_state = client.inbox_state(true).await.unwrap();
    // let inbox_state = client.get_latest_inbox_state("f87420435131ea1b911ad66fbe4b626b107f81955da023d049f8aef6636b8e1b".to_string()).await.unwrap();
    // let inbox_state = client.get_latest_inbox_state("bd03ba1d688c7ababe4e39eb0012a3cff7003e0faef2e164ff95e1ce4db30141".to_string()).await.unwrap();

    // Extract installation IDs from the inbox state
    let installation_ids: Vec<Vec<u8>> = inbox_state
        .installations
        .iter()
        .map(|installation| installation.id.clone())
        .collect();

    assert!(
        !installation_ids.is_empty(),
        "Client should have at least one installation ID"
    );

    // Get key packages for the installation IDs
    let key_package_statuses = client
        .get_key_package_statuses_for_installation_ids(installation_ids.clone())
        .await
        .unwrap();

    // Verify we got results for all installation IDs
    assert_eq!(
        key_package_statuses.len(),
        installation_ids.len(),
        "Should get key package status for each installation ID"
    );

    // Check each key package status
    for (installation_id, key_package_status) in key_package_statuses {
        println!("Installation ID: {:?}", hex::encode(&installation_id));

        if let Some(error) = &key_package_status.validation_error {
            println!("Key package validation error: {}", error);
        } else if let Some(lifetime) = &key_package_status.lifetime {
            let not_before_date =
                chrono::DateTime::<chrono::Utc>::from_timestamp(lifetime.not_before as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| lifetime.not_before.to_string());
            let not_after_date =
                chrono::DateTime::<chrono::Utc>::from_timestamp(lifetime.not_after as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| lifetime.not_after.to_string());

            println!(
                "Key package valid: not_before={} ({}), not_after={} ({})",
                lifetime.not_before, not_before_date, lifetime.not_after, not_after_date
            );
            println!();

            // Verify the lifetime is valid (not expired)
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            assert!(
                lifetime.not_before <= current_time,
                "Key package should be valid now"
            );
            assert!(
                lifetime.not_after > current_time,
                "Key package should not be expired"
            );
        } else {
            println!("No lifetime for this key package")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 5)]
async fn test_get_hmac_keys() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let alix_group = alix
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let hmac_keys = alix_group.get_hmac_keys().unwrap();

    let keys = hmac_keys.get(&alix_group.id()).unwrap();

    assert!(!keys.is_empty());
    assert_eq!(keys.len(), 3);

    for value in keys {
        assert!(!value.key.is_empty());
        assert_eq!(value.key.len(), 42);
        assert!(value.epoch >= 1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_shutdown_is_idempotent() {
    let client = new_test_client().await;
    client.shutdown().await.unwrap();
    // A second call must resolve to Ok(()) without panicking — consumers may
    // call shutdown() defensively on a client they cannot prove is still open.
    client.shutdown().await.unwrap();
}

// verifies: IDENT-072
#[xmtp_common::test(unwrap_try = true)]
async fn register_after_unconfirmed_registration_waits() {
    use xmtp_db::{Fetch, identity::StoredIdentity, prelude::QueryIdentityUpdates};
    use xmtp_mls::utils::test::set_registration_cursor_for_test;
    let wallet = FfiWalletInboxOwner::new();
    let inbox = wallet.identifier().inbox_id(1)?;
    let path = tmp_path();
    let open = async |allow_offline| {
        create_client(
            connect_to_backend_test().await,
            DbOptions::new(Some(path.clone()), None, None, None, None),
            &inbox,
            wallet.identifier(),
            1,
            None,
            None,
            allow_offline,
            None,
            None,
            None,
        )
        .await
        .unwrap()
    };
    let first = open(None).await;
    let signature = first.signature_request().unwrap();
    register_client_with_wallet(&wallet, &first).await;
    let receipt = first
        .inner_client
        .context
        .db()
        .get_latest_sequence_id(&[inbox.as_str()])?[&inbox];
    set_registration_cursor_for_test(&first.inner_client.context.db(), i64::MAX);
    first.inner_client.close().await?;
    drop(first);
    // An offline build must return a local client while the receipt is pending.
    let reopened = open(Some(true)).await;
    assert!(reopened.inner_client.identity().is_ready());
    assert!(
        xmtp_common::time::timeout(
            std::time::Duration::from_millis(200),
            reopened.register_identity(
                signature.clone(),
                Some(crate::FfiVisibilityConfirmationOptions {
                    timeout_ms: Some(0)
                })
            )
        )
        .await
        .is_err()
    );
    let stored: StoredIdentity = reopened.inner_client.context.db().fetch(&())?.unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, Some(i64::MAX));
    set_registration_cursor_for_test(&reopened.inner_client.context.db(), receipt);
    reopened
        .register_identity(
            signature,
            Some(crate::FfiVisibilityConfirmationOptions {
                timeout_ms: Some(0),
            }),
        )
        .await?;
    let stored: StoredIdentity = reopened.inner_client.context.db().fetch(&())?.unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, None);
    reopened.inner_client.close().await?;
}

// verifies: IDENT-072
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_key_creation_waits_until_visible() {
    use xmtp_db::{Fetch, identity::StoredIdentity};
    use xmtp_id::associations::ValidatedLegacySignedPublicKey;
    use xmtp_proto::xmtp::message_contents::{
        Signature, SignedPrivateKey, SignedPublicKey, UnsignedPublicKey, signature,
        signature::WalletEcdsaCompact,
        signed_private_key::{Secp256k1, Union},
        unsigned_public_key::{self, Secp256k1Uncompressed},
    };

    let wallet = FfiWalletInboxOwner::new();
    let created_ns = xmtp_common::rand_u64();
    let secret = alloy::signers::k256::ecdsa::SigningKey::from_slice(
        &xmtp_cryptography::rand::rand_array::<32>(),
    )?;
    let public_key = alloy::signers::k256::ecdsa::VerifyingKey::from(&secret);
    let mut public_key_bytes = Vec::new();
    UnsignedPublicKey {
        created_ns,
        union: Some(unsigned_public_key::Union::Secp256k1Uncompressed(
            Secp256k1Uncompressed {
                bytes: public_key.to_sec1_bytes().to_vec(),
            },
        )),
    }
    .encode(&mut public_key_bytes)?;
    let signed_public_key = wallet.sign(ValidatedLegacySignedPublicKey::text(&public_key_bytes))?;
    let (signature_bytes, recovery_id) = signed_public_key.split_at(64);
    let mut legacy_key = Vec::new();
    SignedPrivateKey {
        created_ns,
        public_key: Some(SignedPublicKey {
            key_bytes: public_key_bytes,
            signature: Some(Signature {
                union: Some(signature::Union::WalletEcdsaCompact(WalletEcdsaCompact {
                    bytes: signature_bytes.to_vec(),
                    recovery: recovery_id[0].into(),
                })),
            }),
        }),
        union: Some(Union::Secp256k1(Secp256k1 {
            bytes: secret.to_bytes().to_vec(),
        })),
    }
    .encode(&mut legacy_key)?;

    let identifier = wallet.identifier();
    let inbox = identifier.inbox_id(0)?;
    let client = create_client(
        connect_to_backend_test().await,
        DbOptions::new(Some(tmp_path()), None, None, None, None),
        &inbox,
        identifier,
        0,
        Some(legacy_key),
        None,
        None,
        None,
        None,
        None,
    )
    .await?;
    assert!(client.inner_client.identity().is_ready());
    assert!(client.inner_client.is_registration_visible()?);
    let stored: StoredIdentity = client.inner_client.context.db().fetch(&())?.unwrap();
    assert_eq!(stored.registration_cursor_sequence_id, None);
    client.inner_client.close().await?;
}

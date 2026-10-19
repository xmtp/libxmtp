//! Heap-canary proofs that libxmtp wipes the copies of secrets it owns.
//! Native only: it installs the heap-canary global allocator.
//! Needs the shared backend (`just backend up`).
#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use xmtp_cryptography::{
    XmtpInstallationCredential,
    secret::heap_canary::{Canary, scan_async},
    utils::generate_local_wallet,
};
use xmtp_db::{StoredSecret, XmtpTestDb, identity::StoredIdentity};
use xmtp_id::{InboxOwner, associations::test_utils::MockSmartContractSignatureVerifier};
use xmtp_mls::{
    Client,
    groups::send_message_opts::SendMessageOpts,
    identity::{Identity, IdentityStrategy},
    utils::{FullXmtpClient, register_client},
};

xmtp_cryptography::install_heap_canary!();

const INSTALLATION_KEY: [u8; 32] = [
    0x3b, 0xd4, 0x61, 0x9e, 0x07, 0xc2, 0x58, 0xaf, 0x14, 0xe9, 0x7d, 0x30, 0x86, 0x4f, 0xb1, 0x2a,
    0xf3, 0x6c, 0x09, 0xd8, 0x45, 0x92, 0x1e, 0xbb, 0x70, 0x2d, 0xe6, 0x53, 0x8a, 0xc7, 0x1f, 0x64,
];

async fn open_client(path: &str, strategy: IdentityStrategy) -> FullXmtpClient {
    Client::builder(strategy)
        .store(xmtp_db::TestDb::create_persistent_store(Some(path.to_owned())).await)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .with_disable_workers(true)
        .default_mls_store()
        .unwrap()
        .local()
        .build()
        .await
        .unwrap()
}

/// Registering stores the identity row; reopening the database fetches it.
/// Neither may free an unwiped copy of the installation key.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn installation_key_is_wiped_on_store_and_load() {
    let wallet = generate_local_wallet();
    let credential = XmtpInstallationCredential::from_bytes(&INSTALLATION_KEY)?;
    let identity =
        Identity::new_inbox_with_installation_keys(wallet.get_identifier()?, 1, credential).await?;
    let path = xmtp_common::tmp_path();

    let ((), hits) = scan_async(&[Canary::new(&INSTALLATION_KEY)], async {
        let client = open_client(&path, IdentityStrategy::ExternalIdentity(identity)).await;
        register_client(&client, &wallet).await;
        let group = client.create_group(None, None).unwrap();
        group
            .send_message(b"first", SendMessageOpts::default())
            .await
            .unwrap();
        let group_id = group.group_id;
        drop(group);
        drop(client);

        let client = open_client(&path, IdentityStrategy::CachedOnly).await;
        let group = client.group(&group_id).unwrap();
        group
            .send_message(b"second", SendMessageOpts::default())
            .await
            .unwrap();
        drop(group);
        drop(client);
        // Let tasks that still hold client state finish and free it.
        xmtp_common::time::sleep(Duration::from_millis(500)).await;
    })
    .await;

    assert_eq!(hits, [0], "freed heap blocks held the installation key");
}

/// No `Debug` output of a type that holds the installation key shows it.
#[xmtp_common::test(unwrap_try = true)]
async fn installation_key_holders_redact_debug() {
    let credential = XmtpInstallationCredential::from_bytes(&INSTALLATION_KEY)?;
    let serialized = xmtp_db::db_serialize(&credential)?;
    let stored = StoredSecret::from(serialized.clone());
    let row = StoredIdentity::new("inbox".to_owned(), serialized, vec![1, 2, 3]);
    let identity = Identity::new_inbox_with_installation_keys(
        generate_local_wallet().get_identifier()?,
        1,
        credential.clone(),
    )
    .await?;

    let forms = [
        hex::encode(INSTALLATION_KEY),
        hex::encode_upper(INSTALLATION_KEY),
        // How `Debug` prints a byte slice or array, anywhere in the list.
        format!("{:?}", &INSTALLATION_KEY[..4])
            .trim_matches(['[', ']'])
            .to_owned(),
    ];
    let cases = [
        ("StoredSecret", format!("{stored:?}")),
        ("StoredIdentity", format!("{row:?}")),
        ("XmtpInstallationCredential", format!("{credential:?}")),
        ("Identity", format!("{identity:?}")),
    ];
    for (name, debug) in cases {
        for form in &forms {
            assert!(
                !debug.contains(form.as_str()),
                "{name} Debug shows the key as {form}: {debug}"
            );
        }
    }
}

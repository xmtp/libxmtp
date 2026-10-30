//! Heap-canary proofs that libxmtp wipes the copies of secrets it owns.
//! Native only: it installs the heap-canary global allocator.
//! Needs the shared backend (`just backend up`).
//!
//! A span or event field that copies a key must free that copy inside the
//! scan. Without a subscriber, CI evaluates almost none: the test macro
//! installs none there, and with none tracing's `log` feature builds an
//! event's fields only for a `log` logger, which CI does not set, and a
//! span's only until the process first sets any dispatcher. So each scan runs
//! under [`traced`] or [`traced_sync`]: a global subscriber that formats
//! every field on every thread when none is set
//! (`install_field_evaluator`), and a trace layer over the flow itself.
// The heap-canary harness is Unix-only (`libc::memmem`).
#![cfg(unix)]

use std::time::Duration;

use prost::Message;

use openmls::prelude::KeyPackageBundle;
use openmls_traits::{OpenMlsProvider, storage::StorageProvider};
use xmtp_common::{NS_IN_HOUR, time::now_ns};
use xmtp_cryptography::{
    XmtpInstallationCredential,
    secret::{
        Secret, Zeroize,
        heap_canary::{Canary, scan, scan_async},
    },
    utils::generate_local_wallet,
};
use xmtp_db::{
    Store, StoredSecret, XmtpMlsStorageProvider, XmtpOpenMlsProviderRef, XmtpTestDb,
    group_intent::{IntentKind, QueryGroupIntent},
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    identity::StoredIdentity,
    sql_key_store::{KEY_PACKAGE_WRAPPER_PRIVATE_KEY, codec::encode_key},
    user_preferences::{RootKeyWrite, StoredUserPreferences, write_root_key},
};
use xmtp_id::{InboxOwner, associations::test_utils::MockSmartContractSignatureVerifier};
use xmtp_mls::{
    Client,
    context::XmtpSharedContext,
    groups::{
        intents::{QueueIntent, UpdateMetadataIntentData},
        send_message_opts::SendMessageOpts,
    },
    identity::{Identity, IdentityStrategy},
    tester,
    utils::{FullXmtpClient, register_client},
};
use xmtp_mls_common::{
    app_data::{
        component_id::ComponentId,
        creation::{InitialGroupKind, initial_dictionary},
        group_root::{check_group_root, group_keys_from_extensions},
    },
    group::GroupMetadataOptions,
    group_keys::{GroupKeys, GroupRoot},
};
use xmtp_proto::xmtp::device_sync::content::{
    DeviceSyncContent, HmacKeyUpdate as HmacKeyUpdateProto,
    PreferenceUpdate as PreferenceUpdateProto, PreferenceUpdates,
    device_sync_content::Content as DeviceSyncContentKind,
    preference_update::Update as UpdateProto,
};
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, EncodedContent};

xmtp_cryptography::install_heap_canary!();

const INSTALLATION_KEY: [u8; 32] = [
    0x3b, 0xd4, 0x61, 0x9e, 0x07, 0xc2, 0x58, 0xaf, 0x14, 0xe9, 0x7d, 0x30, 0x86, 0x4f, 0xb1, 0x2a,
    0xf3, 0x6c, 0x09, 0xd8, 0x45, 0x92, 0x1e, 0xbb, 0x70, 0x2d, 0xe6, 0x53, 0x8a, 0xc7, 0x1f, 0x64,
];

const HMAC_ROOT_KEY: [u8; 42] = [
    0x6a, 0x0f, 0xc3, 0x58, 0x91, 0x2e, 0xd7, 0x44, 0xb9, 0x13, 0x7c, 0xe5, 0x20, 0x8b, 0x56, 0xfa,
    0x37, 0xa0, 0x6d, 0xc8, 0x02, 0x9f, 0x4b, 0xe6, 0x15, 0x73, 0xde, 0x28, 0x84, 0x3a, 0xbf, 0x61,
    0x0c, 0xf9, 0x47, 0x92, 0x2b, 0xd0, 0x7e, 0x35, 0xa8, 0x5c,
];
const SYNCED_HMAC_ROOT_KEY: [u8; 42] = [
    0xd1, 0x4c, 0x87, 0x2a, 0xf0, 0x63, 0x19, 0xbe, 0x55, 0xe2, 0x08, 0x9d, 0x76, 0x31, 0xca, 0x4f,
    0xa3, 0x1e, 0x68, 0xd5, 0x0b, 0x92, 0x3c, 0xf7, 0x40, 0xad, 0x26, 0x81, 0xe9, 0x5b, 0x14, 0xc6,
    0x7f, 0x38, 0xb2, 0x0d, 0xe4, 0x59, 0x96, 0x23, 0xfb, 0x6e,
];

const GROUP_ROOT: [u8; 32] = [
    0x5e, 0xa1, 0x0c, 0x93, 0x27, 0xd8, 0x4b, 0xf6, 0x81, 0x3a, 0xe7, 0x12, 0xc5, 0x6f, 0x98, 0x24,
    0xbd, 0x40, 0x7e, 0x09, 0xf2, 0x63, 0xac, 0x15, 0xd9, 0x36, 0x8b, 0xe0, 0x57, 0x1c, 0xa4, 0x7b,
];

const COMMIT_LOG_SIGNER: [u8; 32] = [
    0x92, 0x3e, 0xd5, 0x08, 0x6c, 0xb1, 0x47, 0xfa, 0x2d, 0x80, 0x5b, 0xe4, 0x19, 0xc6, 0x73, 0x0a,
    0xbe, 0x61, 0x34, 0xd9, 0x8f, 0x02, 0xa7, 0x56, 0xeb, 0x18, 0xcd, 0x45, 0x7a, 0x93, 0x2f, 0xf6,
];

/// Poll `future` under a trace layer and drop the exported spans before
/// returning, so any copy they hold is freed inside the caller's scan. The
/// layer covers what `future` itself polls; a task it spawns reaches the
/// global field evaluator instead.
async fn traced<T>(future: impl std::future::Future<Output = T>) -> T {
    use tracing::Instrument as _;
    xmtp_logging::test_logging::install_field_evaluator();
    let (result, spans) = xmtp_logging::test_logging::with_trace_layer_async(
        true,
        future.instrument(tracing::error_span!("zeroize")),
    )
    .await;
    drop(spans);
    result
}

/// [`traced`] for a closure.
fn traced_sync<T>(run: impl FnOnce() -> T) -> T {
    xmtp_logging::test_logging::install_field_evaluator();
    let (result, spans) = xmtp_logging::test_logging::with_trace_layer(true, || {
        tracing::error_span!("zeroize").in_scope(run)
    });
    drop(spans);
    result
}

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

    let ((), hits) = scan_async(
        &[Canary::new(&INSTALLATION_KEY)],
        traced(async {
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
        }),
    )
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

    // The stored credential is CBOR, which writes each key byte of 24 or
    // more as 0x18 then the byte.
    let cbor_items = |bytes: &[u8]| -> Vec<u8> {
        bytes
            .iter()
            .flat_map(|&byte| {
                if byte >= 0x18 {
                    vec![0x18, byte]
                } else {
                    vec![byte]
                }
            })
            .collect()
    };
    // How `Debug` prints a byte slice or array, anywhere in the list.
    let debug = |bytes: &[u8]| format!("{bytes:?}").trim_matches(['[', ']']).to_owned();
    let forms = [
        hex::encode(INSTALLATION_KEY),
        hex::encode_upper(INSTALLATION_KEY),
        debug(&INSTALLATION_KEY[..4]),
        hex::encode(cbor_items(&INSTALLATION_KEY)),
        debug(&cbor_items(&INSTALLATION_KEY[..4])),
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

/// Opening a Welcome reads the key package's init key and its post-quantum
/// wrapper key. Neither may be freed unwiped.
///
/// Packages advertise the X-Wing wrapper while
/// `CREATE_PQ_KEY_PACKAGE_EXTENSION` is true, so the Welcome takes the
/// post-quantum arm of `find_private_key`. Its Curve25519 arm is proven by the
/// `Zeroizing` return type.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn welcome_keys_are_wiped_when_joining() {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    // Read the keys of the package bo will use, before arming.
    let installation_id = alix.installation_public_key().to_vec();
    let package = alix
        .get_key_packages_for_installation_ids(vec![installation_id.clone()])
        .await?
        .remove(&installation_id)
        .expect("alix published a key package")?;
    let provider = XmtpOpenMlsProviderRef::new(alix.context.mls_storage());
    let hash_ref = package.inner.hash_ref(provider.crypto())?;
    let bundle: KeyPackageBundle = provider
        .storage()
        .key_package(&hash_ref)?
        .expect("alix stores the published bundle");
    let wrapper_key = provider
        .storage()
        .wrapper_private_key(&encode_key(KEY_PACKAGE_WRAPPER_PRIVATE_KEY, &hash_ref)?)?
        .expect("the package has a post-quantum wrapper key");
    let canaries = [
        Canary::new(bundle.init_private_key()),
        Canary::new(&wrapper_key),
    ];
    drop((bundle, wrapper_key));

    let ((), hits) = scan_async(
        &canaries,
        traced(async {
            let group = bo.create_group(None, None).unwrap();
            group.add_members(&[alix.inbox_id()]).await.unwrap();
            let joined = alix.sync_welcomes().await.unwrap();
            assert_eq!(joined.len(), 1);
            drop((group, joined));
            // Let tasks that still hold Welcome state finish and free it.
            xmtp_common::time::sleep(Duration::from_millis(500)).await;
        }),
    )
    .await;

    assert_eq!(hits, [0, 0], "freed heap blocks held a Welcome private key");
}

/// Loading the preferences, deriving a group's HMAC keys, sending a group
/// message, and converting and storing a synced root key must not free an
/// unwiped copy of a root key or a derived key.
///
/// The sync passes are covered by `sync_passes_wipe_stored_root_keys`. A
/// generated root key cannot be a canary;
/// `HmacKey::random_key` returns `Secret`, proven by type.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn hmac_keys_are_wiped_on_load_derive_send_and_sync() {
    tester!(alix, disable_workers);
    let db = alix.context.db();
    write_root_key(
        &db,
        RootKeyWrite::Received {
            key: &HMAC_ROOT_KEY,
            cycled_at_ns: 1,
            bound_ns: i64::MAX,
        },
        xmtp_db::consent_record::Owed::Keep,
    )?;
    let group = alix.create_group(None, None)?;
    // A stack copy of the current epoch's derived key, made before arming.
    let current_key: [u8; 42] = group.hmac_keys(0..=0)?[0].key;
    let synced = UpdateProto::Hmac(HmacKeyUpdateProto {
        key: SYNCED_HMAC_ROOT_KEY.to_vec(),
        cycled_at_ns: i64::MAX - 1,
    });
    let canaries = [
        Canary::new(&HMAC_ROOT_KEY),
        Canary::new(&current_key),
        Canary::new(&SYNCED_HMAC_ROOT_KEY),
    ];

    let ((), hits) = scan_async(
        &canaries,
        traced(async {
            drop(StoredUserPreferences::load(&db).unwrap());
            drop(group.hmac_keys(-1..=1).unwrap());
            group
                .send_message(b"hmac", SendMessageOpts::default())
                .await
                .unwrap();

            // As the receive path does: borrow the decoded key, then wipe it.
            let mut synced = synced;
            let UpdateProto::Hmac(hmac) = &mut synced else {
                panic!("an HMAC update");
            };
            write_root_key(
                &db,
                RootKeyWrite::Received {
                    key: &hmac.key,
                    cycled_at_ns: hmac.cycled_at_ns,
                    bound_ns: i64::MAX,
                },
                xmtp_db::consent_record::Owed::Keep,
            )
            .unwrap();
            hmac.key.zeroize();
            drop(synced);
        }),
    )
    .await;

    assert_eq!(
        hits,
        [0, 0, 0],
        "freed heap blocks held an HMAC root key or derived key"
    );
    let stored = StoredUserPreferences::load(&db)?.hmac_key;
    assert_eq!(
        stored.as_ref().map(StoredSecret::as_slice),
        Some(SYNCED_HMAC_ROOT_KEY.as_slice())
    );
}

/// The `EncodedContent` bytes of a sync message carrying root key `key`.
/// The copies made on the way are wiped: a block freed unwiped before a scan
/// can be reused and freed again inside it.
fn root_key_message(key: &[u8], cycled_at_ns: i64) -> Vec<u8> {
    let mut update = HmacKeyUpdateProto {
        key: key.to_vec(),
        cycled_at_ns,
    };
    let mut content = DeviceSyncContent {
        content: Some(DeviceSyncContentKind::PreferenceUpdates(
            PreferenceUpdates {
                updates: vec![PreferenceUpdateProto {
                    update: Some(UpdateProto::Hmac(update.clone())),
                }],
            },
        )),
    }
    .encode_to_vec();
    update.key.zeroize();
    let mut encoded = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "xmtp.org".to_string(),
            type_id: "application/x-protobuf".to_string(),
            version_major: 1,
            version_minor: 0,
        }),
        content: std::mem::take(&mut content),
        ..Default::default()
    };
    let bytes = encoded.encode_to_vec();
    encoded.content.zeroize();
    bytes
}

/// The outbox and receive passes free no unwiped copy of a root key that a
/// stored sync message holds: an own page that failed for good, an own
/// published page, and a page from an installation the own inbox never
/// held. Encrypting and decrypting a sync message is OpenMLS's, out of reach.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn sync_passes_wipe_stored_root_keys() {
    tester!(alix, disable_workers);
    let sync = alix.device_sync_client();
    let group_id = sync.get_sync_group().await?.group_id;
    // The new sync group's own page goes out before the scan.
    sync.sync_passes_for_test().await?;
    let db = alix.context.db();
    let keys: [[u8; 42]; 3] = std::array::from_fn(|i| {
        let mut key = SYNCED_HMAC_ROOT_KEY;
        key[0] ^= 0x10 << i;
        key
    });
    let cycled_at_ns = now_ns() + NS_IN_HOUR;
    let own = alix.context.installation_id().to_vec();
    let rows = [
        (DeliveryStatus::Failed, own.clone()),
        (DeliveryStatus::Published, own),
        (DeliveryStatus::Published, xmtp_common::rand_vec::<32>()),
    ];
    for (n, ((status, sender), key)) in rows.into_iter().zip(&keys).enumerate() {
        let mut row = StoredGroupMessage {
            id: xmtp_common::rand_vec::<32>(),
            group_id,
            decrypted_message_bytes: root_key_message(key, cycled_at_ns),
            sent_at_ns: now_ns(),
            kind: GroupMessageKind::Application,
            sender_installation_id: sender,
            sender_inbox_id: alix.inbox_id().to_string(),
            delivery_status: status,
            content_type: ContentType::Unknown,
            version_major: 1,
            version_minor: 0,
            authority_id: "xmtp.org".to_string(),
            reference_id: None,
            sequence_id: 1_000 + n as i64,
            envelope_hash: None,
            expiry_ns: None,
            expiry_unknown: false,
            expire_at_ns: None,
            inserted_at_ns: 0,
            should_push: false,
            idempotency_key: n.to_string(),
        };
        row.store(&db)?;
        row.decrypted_message_bytes.zeroize();
    }
    let canaries = keys.each_ref().map(|key| Canary::new(key));

    let ((), hits) = scan_async(
        &canaries,
        traced(async {
            sync.sync_passes_for_test().await.unwrap();
        }),
    )
    .await;

    assert_eq!(hits, [0, 0, 0], "freed heap blocks held a synced root key");
    // The own published page was applied.
    let stored = StoredUserPreferences::load(&db)?.hmac_key;
    assert_eq!(
        stored.as_ref().map(StoredSecret::as_slice),
        Some(keys[1].as_slice())
    );
}

/// Preparing a message whose content holds an HMAC root key, as a
/// device-sync HMAC update does, frees no unwiped copy of the key: the
/// content checks, the stored row, the envelope and the queued intent all
/// wipe it. SQLite's own memory is not the Rust heap; publishing hands the
/// envelope to OpenMLS, which is out of reach.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn preparing_a_sync_message_wipes_its_content() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let content = EncodedContent {
        r#type: Some(ContentTypeId {
            authority_id: "xmtp.org".to_string(),
            type_id: "application/x-protobuf".to_string(),
            version_major: 1,
            version_minor: 0,
        }),
        content: HMAC_ROOT_KEY.to_vec(),
        ..Default::default()
    }
    .encode_to_vec();

    let ((), hits) = scan(&[Canary::new(&HMAC_ROOT_KEY)], || {
        traced_sync(|| {
            group
                .send_message_optimistic(&content, SendMessageOpts::default())
                .unwrap();
        })
    });
    drop(content);

    assert_eq!(hits, [0], "freed heap blocks held the HMAC root key");
}

/// Building an update-commit-log-signer intent, encoding it, queueing it,
/// reading the stored row back, cloning it and decoding it free no unwiped
/// copy of the signer, raw or hex. SQLite's own memory is not the Rust heap.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread")]
async fn commit_log_signer_intent_wipes_its_copies() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let db = alix.context.db();
    let signer = Secret::new(COMMIT_LOG_SIGNER.to_vec());
    let signer_hex = hex::encode(COMMIT_LOG_SIGNER);

    let ((), hits) = scan(&[Canary::new(&COMMIT_LOG_SIGNER)], || {
        traced_sync(|| {
            let intent = UpdateMetadataIntentData::new_update_commit_log_signer(signer);
            let queued = QueueIntent::metadata_update()
                .data(intent)
                .queue(&group)
                .unwrap();
            let rows = db
                .find_group_intents(group.group_id, None, Some(vec![IntentKind::MetadataUpdate]))
                .unwrap();
            let snapshot = queued.clone();
            let decoded =
                UpdateMetadataIntentData::try_from(snapshot.data.as_slice().to_vec()).unwrap();
            assert!(decoded.field_value == signer_hex);
            drop((queued, rows, snapshot, decoded));
        })
    });
    drop(signer_hex);

    assert_eq!(hits, [0], "freed heap blocks held the commit-log signer");
}

/// `Debug` of an update intent shows the field name but no value.
#[xmtp_common::test(unwrap_try = true)]
fn metadata_intent_debug_redacts_values() {
    let intent = UpdateMetadataIntentData::new_update_commit_log_signer(Secret::new(
        COMMIT_LOG_SIGNER.to_vec(),
    ));
    let guarded = UpdateMetadataIntentData::new_update_app_data(
        "private app data".to_owned(),
        Some("prior app data".to_owned()),
    );
    let debug = format!("{intent:?} {guarded:?}");
    assert!(debug.contains("_commit_log_signer"), "{debug}");
    for form in [
        hex::encode(COMMIT_LOG_SIGNER),
        "private app data".to_owned(),
        "prior app data".to_owned(),
    ] {
        assert!(
            !debug.contains(&form),
            "UpdateMetadataIntentData Debug shows {form}: {debug}"
        );
    }
}

/// The epoch-0 dictionary of a new keyed group with `root`. The dictionary is
/// handed to OpenMLS, which owns its copy of the root from then on.
fn keyed_dictionary(root: &GroupRoot) -> openmls::extensions::AppDataDictionary {
    initial_dictionary(
        InitialGroupKind::Group {
            conversation_type: xmtp_proto::types::ConversationType::Group,
            oneshot_message: None,
        },
        &xmtp_mls::groups::group_permissions::PolicySet::default()
            .to_proto()
            .unwrap(),
        &GroupMetadataOptions::default(),
        &hex::encode([1u8; 32]),
        Some(root),
        &[],
    )
    .unwrap()
}

/// Creating a keyed group reads its root, derives the group id from it, and
/// writes it into the dictionary OpenMLS stores. None of these steps frees an
/// unwiped copy of the root. The dictionary is OpenMLS state, so it is freed
/// after the scan; OpenMLS frees its own group-context copies unwiped, as it
/// does the commit-log signer's.
#[xmtp_common::test(unwrap_try = true)]
fn creating_a_keyed_group_wipes_its_root() {
    let ((dictionary, group_id), hits) = scan(&[Canary::new(&GROUP_ROOT)], || {
        traced_sync(|| {
            let root = GroupRoot::from_bytes(&GROUP_ROOT).unwrap();
            let group_id = GroupKeys::derive(&root).group_id();
            let dictionary = keyed_dictionary(&root);
            drop(root);
            (dictionary, group_id)
        })
    });
    assert_eq!(hits, [0], "freed heap blocks held the group root");
    assert_eq!(
        dictionary.get(&ComponentId::GROUP_ROOT_KEY.as_u16()),
        Some(GROUP_ROOT.as_slice())
    );
    assert_eq!(
        group_id,
        GroupKeys::derive(&GroupRoot::from_bytes(&GROUP_ROOT)?).group_id()
    );
}

/// A joiner reads the root from the Welcome's group context and checks that
/// it names the group, on success and on failure. Neither frees an unwiped
/// copy of the root.
#[xmtp_common::test(unwrap_try = true)]
fn checking_a_welcome_root_wipes_it() {
    use openmls::extensions::{AppDataDictionaryExtension, Extension, Extensions};
    let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(keyed_dictionary(&GroupRoot::from_bytes(&GROUP_ROOT)?)),
    )])?;
    let group_id = GroupKeys::derive(&GroupRoot::from_bytes(&GROUP_ROOT)?).group_id();

    let ((), hits) = scan(&[Canary::new(&GROUP_ROOT)], || {
        traced_sync(|| {
            let keys = check_group_root(&extensions, &group_id).unwrap();
            assert_eq!(keys.group_id(), group_id);
            drop(keys);
            let error = check_group_root(&extensions, &[0u8; 32]).unwrap_err();
            tracing::warn!("rejected: {error} {error:?}");
            drop(group_keys_from_extensions(&extensions).unwrap());
        })
    });
    drop(extensions);

    assert_eq!(hits, [0], "freed heap blocks held the group root");
}

/// The root and each key of the group key table, derived here with HKDF from
/// the table's salt and labels rather than by the code under test.
fn group_key_table(root: &[u8; 32]) -> Vec<[u8; 32]> {
    let hkdf = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"xmtp group root"), root);
    let mut secrets = vec![*root];
    for info in [
        "xmtp group signing key",
        "xmtp commit log signing key",
        "xmtp commit log encryption key",
        "xmtp commit log nonce key",
    ] {
        let mut key = [0u8; 32];
        hkdf.expand(info.as_bytes(), &mut key).unwrap();
        secrets.push(key);
    }
    secrets
}

/// Signing a group message at preparation reads the root from the group
/// context, derives the group keys, and signs: `check_group_root`, then
/// `sign_group_message`, the two calls `prepare_group_envelopes_in` makes.
/// Neither frees an unwiped copy of the root or of any derived key. The
/// extensions are OpenMLS state, so they are freed after the scan.
#[xmtp_common::test(unwrap_try = true)]
fn signing_a_group_message_wipes_the_root_and_its_keys() {
    use openmls::extensions::{AppDataDictionaryExtension, Extension, Extensions};
    use xmtp_mls_common::group_keys::sign_group_message;

    let secrets = group_key_table(&GROUP_ROOT);
    let canaries: Vec<Canary> = secrets.iter().map(|secret| Canary::new(secret)).collect();
    let extensions = Extensions::from_vec(vec![Extension::AppDataDictionary(
        AppDataDictionaryExtension::new(keyed_dictionary(&GroupRoot::from_bytes(&GROUP_ROOT)?)),
    )])?;
    let group_id = GroupKeys::derive(&GroupRoot::from_bytes(&GROUP_ROOT)?).group_id();
    let mut message = xmtp_proto::backend_v1::GroupMessage {
        data: b"signed at preparation".to_vec(),
        sender_hmac: vec![0x42; 32],
        should_push: true,
        signature: Vec::with_capacity(64),
    };

    let ((), hits) = scan(&canaries, || {
        traced_sync(|| {
            let keys = check_group_root(&extensions, &group_id).unwrap();
            sign_group_message(&keys, &mut message).unwrap();
            drop(keys);
        })
    });
    assert_eq!(message.signature.len(), 64);
    drop((extensions, canaries, secrets));

    assert_eq!(
        hits, [0; 5],
        "freed heap blocks held the root or a group key"
    );
}

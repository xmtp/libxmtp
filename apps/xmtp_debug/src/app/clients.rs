//! Different ways to create a [`crate::DbgClient`]

use std::collections::HashMap;

use super::*;
use crate::XDBG_ID_NONCE;
use crate::app::store::Database;
use crate::app::store::IdentityStore;
use crate::app::store::RandomDatabase;
use crate::app::types::*;
use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::{WrapErr, eyre};
use tokio::sync::Mutex;
use xmtp_api_d14n::MessageBackendBuilder;
use xmtp_db::prelude::Pragmas;
use xmtp_db::{NativeDb, XmtpDb};
use xmtp_mls::builder::DeviceSyncMode;
use xmtp_mls::cursor_store::SqliteCursorStore;

pub async fn new_unregistered_client(
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    new_unregistered_client_for(wallet, App::network()).await
}

/// Like [`new_unregistered_client`] but targets the supplied backend
/// instead of the process-global `App::network()`. Use this when one
/// xdbg run needs to talk to more than one network (e.g. the v3→d14n
/// identity-continuity test).
pub async fn new_unregistered_client_for(
    wallet: Option<&types::EthereumWallet>,
    backend: &crate::args::BackendOpts,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().into_alloy()
    } else {
        generate_wallet().into_alloy()
    };
    new_client_inner(&local_wallet, None, backend).await
}

/// Create a new client + Identity
pub async fn temp_client(wallet: Option<&types::EthereumWallet>) -> Result<crate::DbgClient> {
    temp_client_for(wallet, App::network()).await
}

/// Like [`temp_client`] but targets the supplied backend. SQLite file
/// is bucketed by `backend.hash()` so the same wallet on different
/// networks resolves to different on-disk databases.
pub async fn temp_client_for(
    wallet: Option<&types::EthereumWallet>,
    backend: &crate::args::BackendOpts,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().into_alloy()
    } else {
        generate_wallet().into_alloy()
    };

    let tmp_dir = (*crate::constants::TMPDIR).path();
    let public = local_wallet.get_identifier()?;
    let network = backend.hash();
    let name = format!("{public}:{network}.db3");

    new_client_inner(
        &local_wallet,
        Some(tmp_dir.to_path_buf().join(name)),
        backend,
    )
    .await
}

/// Get the XMTP Client from an [`Identity`]
pub fn client_from_identity(identity: &Identity) -> Result<crate::DbgClient> {
    // Use the identity's stored version_hash so cross-version loads
    // (under non-strict mode) find the correct SQLite partition. In
    // strict mode, all loaded identities have version_hash == this
    // binary's hash, so db_path_for and db_path agree.
    let path = identity.db_path_for(identity.version_hash)?;
    debug!(
        inbox_id = hex::encode(identity.inbox_id),
        db_path = %path.display(),
        "creating client from identity"
    );
    existing_client_inner(path)
}

/// Create a new client + Identity & register it
async fn new_client_inner(
    wallet: &PrivateKeySigner,
    db_path: Option<PathBuf>,
    network: &crate::args::BackendOpts,
) -> Result<crate::DbgClient> {
    let api = network.client_bundle()?;
    let sync_api = network.client_bundle()?;
    let ident = wallet.get_identifier()?;
    let inbox_id = ident.inbox_id(XDBG_ID_NONCE)?;

    let dir = if let Some(p) = db_path {
        p
    } else {
        let dir = crate::app::App::db_directory()?;
        let db_name = format!("{inbox_id}:{}.db3", network.hash());
        dir.join(db_name)
    };
    let path = dir
        .into_os_string()
        .into_string()
        .map_err(|_| eyre::eyre!("Conversion failed from OsString"))?;
    let db = NativeDb::builder()
        .max_pool_size(2)
        .min_pool_size(0)
        .persistent(path)
        .build_unencrypted()?;
    db.db().set_sqlcipher_log("NONE")?;
    let db = EncryptedMessageStore::new(db)?;
    let cursor_store = Arc::new(SqliteCursorStore::new(db.db()));
    let mut backend = MessageBackendBuilder::default();
    backend.cursor_store(cursor_store);
    let api = backend.clone().from_bundle(api)?;
    let sync_api = backend.from_bundle(sync_api)?;

    let client = xmtp_mls::Client::builder(IdentityStrategy::new(
        inbox_id,
        wallet.get_identifier()?,
        XDBG_ID_NONCE,
        None,
    ))
    .api_clients(api, sync_api)
    .store(db)
    .default_mls_store()?
    .with_remote_verifier()?
    .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled))
    .build()
    .await?;

    Ok(client)
}

pub async fn register_client(client: &crate::DbgClient, owner: impl InboxOwner) -> Result<()> {
    let signature_request = client.context.signature_request();
    let ident = owner.get_identifier()?;

    trace!(
        inbox_id = client.inbox_id(),
        ident = format!("{ident:?}"),
        installation_id = hex::encode(client.installation_public_key()),
        "registering client"
    );
    if let Some(mut req) = signature_request {
        let signature_text = req.signature_text();
        let unverified_signature = owner.sign(&signature_text)?;
        req.add_signature(unverified_signature, client.scw_verifier())
            .await?;

        client.register_identity(req).await?;
    } else {
        warn!(ident = format!("{ident:?}"), "Signature request empty!");
    }

    Ok(())
}

/// Create a new client + Identity
fn existing_client_inner(db_path: PathBuf) -> Result<crate::DbgClient> {
    existing_client_inner_for(db_path, App::network())
}

fn existing_client_inner_for(
    db_path: PathBuf,
    network: &crate::args::BackendOpts,
) -> Result<crate::DbgClient> {
    let api = network.client_bundle()?;
    let sync_api = network.client_bundle()?;
    let path = db_path.clone().into_os_string().into_string().unwrap();

    let db = NativeDb::builder()
        .max_pool_size(2)
        // min_pool_size(0): don't eagerly maintain idle connections, since xdbg loads 100+
        // clients and each idle connection consumes a file descriptor (see #3231).
        .min_pool_size(0)
        .persistent(path)
        .build_unencrypted()
        .wrap_err(format!(
            "tried to open sqlite database file@{}",
            db_path.to_string_lossy()
        ))?;
    db.db().set_sqlcipher_log("NONE")?;
    let store = EncryptedMessageStore::new(db).inspect_err(|e| {
        error!(db_path = %(&db_path.as_path().display()), "{e}");
    })?;
    let cursor_store = Arc::new(SqliteCursorStore::new(store.db()));
    let mut backend = MessageBackendBuilder::default();
    backend.cursor_store(cursor_store);
    let api = backend.clone().from_bundle(api)?;
    let sync_api = backend.from_bundle(sync_api)?;

    let client = xmtp_mls::Client::builder(IdentityStrategy::CachedOnly)
        .api_clients(api, sync_api)
        .with_remote_verifier()?
        .store(store)
        .default_mls_store()?
        .with_device_sync_worker_mode(Some(DeviceSyncMode::Disabled))
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build_offline()?;

    Ok(client)
}

/// Loads all identities. Honors `App::strict_versioning()`: when set,
/// reads only this binary's version partition; otherwise reads every
/// version on the network.
pub fn load_all_identities(
    store: &IdentityStore<'static>,
) -> Result<Arc<HashMap<InboxId, Mutex<crate::DbgClient>>>> {
    let identities: Vec<Identity> = if crate::app::App::strict_versioning() {
        store
            .load_for_version(crate::app::App::current_version_hash())?
            .ok_or(eyre!("no identities in store, try generating some"))?
            .map(|g| g.value())
            .collect()
    } else {
        store
            .load()?
            .ok_or(eyre!("no identities in store, try generating some"))?
            .map(|g| g.value())
            .collect()
    };
    let now = std::time::Instant::now();
    let mut clients = HashMap::new();
    let mut skipped = 0u32;
    for identity in identities {
        let inbox_id = identity.inbox_id;
        match client_from_identity(&identity) {
            Ok(client) => {
                clients.insert(inbox_id, Mutex::new(client));
            }
            Err(e) => {
                skipped += 1;
                warn!(
                    inbox_id = hex::encode(inbox_id),
                    loaded = clients.len(),
                    skipped,
                    "skipping client that failed to load: {e:#}"
                );
            }
        }
    }
    if clients.is_empty() {
        eyre::bail!("all clients failed to load");
    }
    tracing::info!(
        "took {:?} to load {} clients (skipped {})",
        now.elapsed(),
        clients.len(),
        skipped
    );
    Ok(Arc::new(clients))
}

pub fn load_n_identities(
    store: &IdentityStore<'static>,
    n: usize,
) -> Result<Arc<HashMap<InboxId, Arc<Mutex<crate::DbgClient>>>>> {
    let mut rng = rand::rng();
    let now = std::time::Instant::now();

    let identities: Vec<Identity> = if crate::app::App::strict_versioning() {
        // Strict: pool from this version's partition only, sample n.
        use rand::seq::SliceRandom;
        let mut pool: Vec<Identity> = store
            .load_for_version(crate::app::App::current_version_hash())?
            .ok_or(eyre!("no identities in store, try generating some"))?
            .map(|g| g.value())
            .collect();
        pool.shuffle(&mut rng);
        pool.truncate(n);
        pool
    } else {
        // Non-strict: existing random_n path.
        store
            .random_n(&mut rng, n)?
            .into_iter()
            .map(|g| g.value())
            .collect()
    };

    tracing::info!(
        "loaded {} identities in {:?}",
        identities.len(),
        now.elapsed()
    );
    let now = std::time::Instant::now();
    let clients = identities
        .into_iter()
        .map(|id| {
            Ok::<_, eyre::Report>((
                id.inbox_id,
                Arc::new(Mutex::new(client_from_identity(&id)?)),
            ))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    tracing::info!(
        "took {:?} to load {} xmtp clients",
        now.elapsed(),
        clients.len()
    );
    Ok(Arc::new(clients))
}

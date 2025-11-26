//! Different ways to create a [`crate::DbgClient`]

use std::collections::HashMap;

use super::*;
use crate::XDBG_ID_NONCE;
use crate::app::store::Database;
use crate::app::store::IdentityStore;
use crate::app::store::RandomDatabase;
use crate::app::types::*;
use alloy::signers::local::PrivateKeySigner;
use color_eyre::eyre::{WrapErr, eyre};
use tokio::sync::Mutex;
use xmtp_db::prelude::Pragmas;
use xmtp_db::{NativeDb, XmtpDb};
use xmtp_mls::builder::SyncWorkerMode;

pub async fn new_unregistered_client(
    network: &args::BackendOpts,
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().into_alloy()
    } else {
        generate_wallet().into_alloy()
    };
    new_client_inner(network, &local_wallet, None).await
}

/// Create a new client + Identity
pub async fn temp_client(
    network: &args::BackendOpts,
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().into_alloy()
    } else {
        generate_wallet().into_alloy()
    };

    let tmp_dir = (*crate::constants::TMPDIR).path();
    let public = local_wallet.get_identifier()?;
    let name = format!("{public}:{}.db3", u64::from(network));

    new_client_inner(
        network,
        &local_wallet,
        Some(tmp_dir.to_path_buf().join(name)),
    )
    .await
}

/// Get the XMTP Client from an [`Identity`]
pub fn client_from_identity(
    identity: &Identity,
    network: &args::BackendOpts,
) -> Result<crate::DbgClient> {
    let path = identity.db_path(network)?;
    debug!(
        inbox_id = hex::encode(identity.inbox_id),
        db_path = %path.display(),
        "creating client from identity"
    );
    existing_client_inner(network, path)
}

/// Create a new client + Identity & register it
async fn new_client_inner(
    network: &args::BackendOpts,
    wallet: &PrivateKeySigner,
    db_path: Option<PathBuf>,
) -> Result<crate::DbgClient> {
    let api = network.connect()?;

    let ident = wallet.get_identifier()?;
    let inbox_id = ident.inbox_id(XDBG_ID_NONCE)?;

    let dir = if let Some(p) = db_path {
        p
    } else {
        let dir = crate::app::App::db_directory(network)?;
        let db_name = format!("{inbox_id}:{}.db3", u64::from(network));
        dir.join(db_name)
    };
    let path = dir
        .into_os_string()
        .into_string()
        .map_err(|_| eyre::eyre!("Conversion failed from OsString"))?;
    let db = NativeDb::new_unencrypted(&StorageOption::Persistent(path))?;
    db.db().set_sqlcipher_log("NONE")?;
    let client = xmtp_mls::Client::builder(IdentityStrategy::new(
        inbox_id,
        wallet.get_identifier()?,
        XDBG_ID_NONCE,
        None,
    ))
    .api_clients(api.clone(), api)
    .store(EncryptedMessageStore::new(db)?)
    .default_mls_store()?
    .with_remote_verifier()?
    .with_device_sync_worker_mode(Some(SyncWorkerMode::Disabled))
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
fn existing_client_inner(
    network: &args::BackendOpts,
    db_path: PathBuf,
) -> Result<crate::DbgClient> {
    let api = network.connect()?;

    let db = xmtp_db::NativeDb::new_unencrypted(&StorageOption::Persistent(
        db_path.clone().into_os_string().into_string().unwrap(),
    ))
    .wrap_err(format!(
        "tried to open sqlite database file@{}",
        db_path.to_string_lossy()
    ))?;
    db.db().set_sqlcipher_log("NONE")?;
    let store = EncryptedMessageStore::new(db);

    if let Err(e) = &store {
        error!(db_path = %(&db_path.as_path().display()), "{e}");
    }
    let client = xmtp_mls::Client::builder(IdentityStrategy::CachedOnly)
        .api_clients(api.clone(), api)
        .with_remote_verifier()?
        .store(store?)
        .default_mls_store()?
        .with_device_sync_worker_mode(Some(SyncWorkerMode::Disabled))
        .with_allow_offline(Some(true))
        .build_offline()?;

    Ok(client)
}

/// Loads all identities
pub fn load_all_identities(
    store: &IdentityStore<'static>,
    network: &args::BackendOpts,
) -> Result<Arc<HashMap<InboxId, Mutex<crate::DbgClient>>>> {
    let identities = store
        .load(u64::from(network))?
        .ok_or(eyre!("no identities in store, try generating some"))?;
    let now = std::time::Instant::now();
    let mut clients_len = 0;
    let clients = identities
        .map(move |i| {
            let item = (
                i.value().inbox_id,
                Mutex::new(client_from_identity(&i.value(), network).wrap_err(format!(
                    "failed to load client for {}, {} other clients succeeded",
                    hex::encode(i.value().inbox_id),
                    clients_len
                ))?),
            );
            clients_len += 1;
            Ok::<_, eyre::Report>(item)
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    tracing::info!("took {:?} to load {} clients", now.elapsed(), clients.len());
    Ok(Arc::new(clients))
}

pub fn load_n_identities(
    store: &IdentityStore<'static>,
    network: &args::BackendOpts,
    n: usize,
) -> Result<Arc<HashMap<InboxId, Arc<Mutex<crate::DbgClient>>>>> {
    let mut rng = rand::thread_rng();
    let now = std::time::Instant::now();
    let identities = store.random_n(u64::from(network), &mut rng, n)?;
    tracing::info!("loaded {} identities in {:?}", n, now.elapsed());
    let now = std::time::Instant::now();
    let clients = identities
        .into_iter()
        .map(move |i| {
            let id = i.value();
            Ok::<_, eyre::Report>((
                id.inbox_id,
                Arc::new(Mutex::new(client_from_identity(&id, network)?)),
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

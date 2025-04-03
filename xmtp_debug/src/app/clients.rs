//! Different ways to create a [`crate::DbgClient`]

use super::*;
use crate::app::types::*;
use color_eyre::eyre;

pub async fn new_registered_client(
    network: args::BackendOpts,
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().into_ethers()
    } else {
        generate_wallet().into_ethers()
    };
    new_client_inner(network, &local_wallet, None).await
}

/// Create a new client + Identity
pub async fn temp_client(
    network: &args::BackendOpts,
    wallet: Option<&types::EthereumWallet>,
) -> Result<crate::DbgClient> {
    let local_wallet = if let Some(w) = wallet {
        w.clone().into_ethers()
    } else {
        generate_wallet().into_ethers()
    };

    let tmp_dir = (*crate::constants::TMPDIR).path();
    let public = local_wallet.get_identifier()?;
    let name = format!("{public}:{}.db3", u64::from(network));

    new_client_inner(
        network.clone(),
        &local_wallet,
        Some(tmp_dir.to_path_buf().join(name)),
    )
    .await
}

pub async fn client_from_identity(
    identity: &Identity,
    network: &args::BackendOpts,
) -> Result<crate::DbgClient> {
    let path = identity.db_path(network)?;
    debug!(
        inbox_id = hex::encode(identity.inbox_id),
        db_path = %path.display(),
        "creating client from identity"
    );
    existing_client_inner(network, path).await
}

/// Create a new client + Identity & register it
async fn new_client_inner(
    network: args::BackendOpts,
    wallet: &LocalWallet,
    db_path: Option<PathBuf>,
) -> Result<crate::DbgClient> {
    let api = network.connect().await?;

    let nonce = 1;
    let ident = wallet.get_identifier()?;
    let inbox_id = ident.inbox_id(nonce)?;

    let dir = if let Some(p) = db_path {
        p
    } else {
        let dir = crate::app::App::db_directory(&network)?;
        let db_name = format!("{inbox_id}:{}.db3", u64::from(network));
        dir.join(db_name)
    };

    let client = xmtp_mls::Client::builder(IdentityStrategy::new(
        inbox_id,
        wallet.get_identifier()?,
        nonce,
        None,
    ))
    .api_client(api)
    .with_remote_verifier()?
    .store(
        EncryptedMessageStore::new(
            StorageOption::Persistent(
                dir.into_os_string()
                    .into_string()
                    .map_err(|_| eyre::eyre!("Conversion failed from OsString"))?,
            ),
            [0u8; 32],
        )
        .await?,
    )
    .build()
    .await?;

    register_client(&client, wallet).await?;

    Ok(client)
}

pub async fn register_client(client: &crate::DbgClient, owner: impl InboxOwner) -> Result<()> {
    let signature_request = client.context().signature_request();
    let ident = owner.get_identifier()?;

    trace!(
        inbox_id = client.inbox_id(),
        ident = format!("{ident:?}"),
        installation_id = hex::encode(client.installation_public_key()),
        "registering client"
    );
    if let Some(mut req) = signature_request {
        let signature_text = req.signature_text();
        let unverified_signature = UnverifiedSignature::RecoverableEcdsa(
            UnverifiedRecoverableEcdsaSignature::new(owner.sign(&signature_text)?.into()),
        );
        req.add_signature(unverified_signature, client.scw_verifier())
            .await?;

        client.register_identity(req).await?;
    } else {
        warn!(ident = format!("{ident:?}"), "Signature request empty!");
    }

    Ok(())
}

/// Create a new client + Identity
async fn existing_client_inner(
    network: &args::BackendOpts,
    db_path: PathBuf,
) -> Result<crate::DbgClient> {
    let api = network.connect().await?;

    let store = EncryptedMessageStore::new(
        StorageOption::Persistent(db_path.clone().into_os_string().into_string().unwrap()),
        [0u8; 32],
    )
    .await;
    if let Err(e) = &store {
        error!(db_path = %(&db_path.as_path().display()), "{e}");
    }
    let client = xmtp_mls::Client::builder(IdentityStrategy::CachedOnly)
        .api_client(api)
        .with_remote_verifier()?
        .store(store?)
        .build()
        .await?;

    Ok(client)
}

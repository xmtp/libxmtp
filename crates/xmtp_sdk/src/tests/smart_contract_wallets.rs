use super::*;

use std::collections::HashMap;

use alloy::{
    dyn_abi::SolType,
    primitives::{U256, eip191_hash_message},
    providers::{DynProvider, Provider},
    signers::Signer as _,
};
use xmtp_id::utils::test::{
    CoinbaseSmartWallet::CoinbaseSmartWalletInstance, SignatureWithNonce, SmartWalletContext,
    docker_smart_wallet,
};

/// A Coinbase smart wallet deployed on the local anvil chain, signing through
/// one of its owners. The backend verifies its ERC-1271 signatures on chain.
struct ContractWalletSigner {
    owner: PrivateKeySigner,
    wallet: CoinbaseSmartWalletInstance<DynProvider>,
    chain_id: u64,
}

impl ContractWalletSigner {
    fn address(&self) -> String {
        self.wallet.address().to_string().to_lowercase()
    }
}

#[xmtp_common::async_trait]
impl Signer for ContractWalletSigner {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        Ok(PublicIdentity {
            identifier: self.address(),
            kind: PublicIdentityKind::Ethereum,
        })
    }

    async fn kind(&self) -> Result<SignerKind, SignerError> {
        Ok(SignerKind::Scw {
            chain_id: self.chain_id,
            block_number: None,
        })
    }

    async fn sign(&self, request: SigningRequest) -> Result<Signature, SignerError> {
        let hash = eip191_hash_message(request.text);
        let replay_safe = self
            .wallet
            .replaySafeHash(hash)
            .call()
            .await
            .map_err(|_| SignerError::Failed)?;
        let signed = self
            .owner
            .sign_hash(&replay_safe)
            .await
            .map_err(|_| SignerError::Failed)?;
        Ok(Signature::Scw {
            bytes: SignatureWithNonce::abi_encode(&(U256::from(0), signed.as_bytes().to_vec())),
            address: self.address(),
            chain_id: self.chain_id,
            block_number: None,
        })
    }
}

fn text_of(message: &crate::Message) -> Option<&str> {
    match &message.0.content {
        MessageContent::Text(text) => Some(text),
        _ => None,
    }
}

// A smart contract wallet creates a client through the public signer
// callback. The client is built again from storage, checks reachability both
// ways, and sends a group message that an EOA member receives.
#[xmtp_common::test(unwrap_try = true)]
async fn smart_contract_wallet_signer_creates_builds_and_messages() {
    let SmartWalletContext {
        factory,
        sw,
        owner0,
        ..
    } = docker_smart_wallet::default().await;
    let chain_id = factory.provider().get_chain_id().await?;
    let signer = Arc::new(ContractWalletSigner {
        owner: owner0,
        wallet: sw,
        chain_id,
    });
    let identity = signer.identity().await?;

    let root = temp_root("smart-contract-wallet");
    std::fs::create_dir_all(&root)?;
    let mut settings = options();
    settings.storage.location = explicit_location(&root.join("wallet.db3"));
    let created = Client::create(signer.clone(), settings.clone()).await?;
    let inbox_id = created.inbox_id();
    assert_eq!(created.identity().identifier, identity.identifier);
    created.end().await?;

    let wallet = Client::build(identity.clone(), settings, Some(inbox_id.clone())).await?;
    assert_eq!(wallet.inbox_id(), inbox_id);
    let state = wallet.inbox_state(true).await?;
    assert_eq!(state.recovery_identity.identifier, identity.identifier);
    assert!(matches!(
        state.creation_signature_kind,
        Some(crate::SignatureKind::Erc1271)
    ));

    let peer = Client::create(crate::generate_local_signer().await, options()).await?;
    let unknown = signer::identity(crate::generate_local_signer().await).await?;
    let key =
        |identity: &PublicIdentity| format!("ethereum:{}", identity.identifier.to_lowercase());
    assert_eq!(
        wallet
            .can_message(vec![peer.identity(), unknown.clone()])
            .await?,
        HashMap::from([(key(&peer.identity()), true), (key(&unknown), false)])
    );
    assert_eq!(
        peer.can_message(vec![identity.clone()]).await?,
        HashMap::from([(key(&identity), true)])
    );

    let group = wallet
        .conversations()
        .create_group(vec![peer.inbox_id()], None)
        .await?;
    let sent = group
        .send_text("smart contract wallet message".into(), None)
        .await?;
    peer.conversations().sync_all(None).await?;
    let received = peer
        .conversations()
        .get_message_by_id(sent)
        .await?
        .expect("the peer received the message");
    assert_eq!(text_of(&received), Some("smart contract wallet message"));
    assert_eq!(received.0.sender_inbox_id, inbox_id);

    wallet.end().await?;
    peer.end().await?;
    std::fs::remove_dir_all(root)?;
}

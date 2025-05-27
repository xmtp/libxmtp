use crate::{client::Client, identity::IdentityStrategy};
use alloy::{dyn_abi::SolType, primitives::U256, providers::Provider, signers::Signer};
use xmtp_id::associations::Identifier;
use xmtp_id::utils::test::{docker_smart_wallet, SmartWalletContext};
use xmtp_id::{
    associations::{unverified::NewUnverifiedSmartContractWalletSignature, AccountId},
    utils::test::SignatureWithNonce,
};

#[rstest::rstest]
#[tokio::test]
async fn test_remote_is_valid_signature(#[future] docker_smart_wallet: SmartWalletContext) {
    let SmartWalletContext {
        factory,
        owner0: wallet,
        sw,
        sw_address,
        ..
    } = docker_smart_wallet.await;
    let provider = factory.provider();
    let chain_id = provider.get_chain_id().await.unwrap();
    let account_address = format!("{sw_address}");
    let ident = Identifier::eth(&account_address).unwrap();
    let account_id = AccountId::new_evm(chain_id, account_address.clone());

    let identity_strategy = IdentityStrategy::new(
        ident.inbox_id(0).unwrap(),
        Identifier::eth(account_address).unwrap(),
        0,
        None,
    );

    let xmtp_client = Client::builder(identity_strategy)
        .temp_store()
        .await
        .local_client()
        .await
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();

    let mut signature_request = xmtp_client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let hash_to_sign = alloy::primitives::eip191_hash_message(signature_text);
    let rsh = sw.replaySafeHash(hash_to_sign).call().await.unwrap();
    let signed_hash = wallet.sign_hash(&rsh).await.unwrap().as_bytes().to_vec();
    let signature_bytes = SignatureWithNonce::abi_encode(&(U256::from(0), signed_hash));

    signature_request
        .add_new_unverified_smart_contract_signature(
            NewUnverifiedSmartContractWalletSignature::new(
                signature_bytes.to_vec(),
                account_id,
                None,
            ),
            &xmtp_client.scw_verifier(),
        )
        .await
        .unwrap();

    xmtp_client
        .register_identity(signature_request)
        .await
        .unwrap();
}

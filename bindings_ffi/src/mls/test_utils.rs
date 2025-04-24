use std::sync::Arc;

use ethers::signers::LocalWallet;
use xmtp_common::tmp_path;
use xmtp_id::InboxOwner;
use xmtp_mls::utils::test::tester_utils::*;

use crate::inbox_owner::FfiInboxOwner;

use super::{tests::FfiWalletInboxOwner, *};

pub trait LocalBuilder<Owner>
where
    Owner: InboxOwner + Clone,
{
    async fn build(&self) -> Tester<Owner, Arc<FfiXmtpClient>>;
    async fn build_no_panic(&self) -> Result<Tester<Owner, Arc<FfiXmtpClient>>, GenericError>;
}
impl LocalBuilder<LocalWallet> for TesterBuilder<LocalWallet> {
    async fn build(&self) -> Tester<LocalWallet, Arc<FfiXmtpClient>> {
        self.build_no_panic().await.unwrap()
    }

    // Will not panic on registering identity. Will still panic on just about everything else.
    async fn build_no_panic(
        &self,
    ) -> Result<Tester<LocalWallet, Arc<FfiXmtpClient>>, GenericError> {
        let client = create_raw_client(self).await;
        let owner = FfiWalletInboxOwner::with_wallet(self.owner.clone());
        let signature_request = client.signature_request().unwrap();
        signature_request
            .add_ecdsa_signature(
                owner
                    .sign(signature_request.signature_text().await.unwrap())
                    .unwrap(),
            )
            .await?;
        client.register_identity(signature_request).await?;

        let provider = client.inner_client.mls_provider()?;
        let worker = client.inner_client.worker_handle();

        if let Some(worker) = &worker {
            if self.wait_for_init {
                worker.wait_for_init().await.unwrap();
            }
        }

        Ok(Tester {
            builder: self.clone(),
            client,
            provider: Arc::new(provider),
            worker,
        })
    }
}
impl LocalBuilder<PasskeyUser> for TesterBuilder<PasskeyUser> {
    async fn build(&self) -> Tester<PasskeyUser, Arc<FfiXmtpClient>> {
        self.build_no_panic().await.unwrap()
    }

    async fn build_no_panic(
        &self,
    ) -> Result<Tester<PasskeyUser, Arc<FfiXmtpClient>>, GenericError> {
        let client = create_raw_client(self).await;
        let signature_request = client.signature_request().unwrap();
        let text = signature_request.signature_text().await.unwrap();
        let UnverifiedSignature::Passkey(signature) = self.owner.sign(&text).unwrap() else {
            unreachable!("Passkeys only provide passkey signatures.");
        };

        signature_request
            .add_passkey_signature(FfiPasskeySignature {
                authenticator_data: signature.authenticator_data,
                client_data_json: signature.client_data_json,
                public_key: signature.public_key,
                signature: signature.signature,
            })
            .await
            .unwrap();
        client.register_identity(signature_request).await?;

        let provider = client.inner_client.mls_provider().unwrap();
        let worker = client.inner_client.worker_handle();

        if let Some(worker) = &worker {
            if self.wait_for_init {
                worker.wait_for_init().await.unwrap();
            }
        }

        Ok(Tester {
            builder: self.clone(),
            client,
            provider: Arc::new(provider),
            worker,
        })
    }
}

pub trait LocalTester {
    async fn new() -> Tester<LocalWallet, Arc<FfiXmtpClient>>;
    #[allow(unused)]
    async fn new_passkey() -> Tester<PasskeyUser, Arc<FfiXmtpClient>>;

    fn builder() -> TesterBuilder<LocalWallet>;
}
impl LocalTester for Tester<LocalWallet, Arc<FfiXmtpClient>> {
    async fn new() -> Tester<LocalWallet, Arc<FfiXmtpClient>> {
        TesterBuilder::new().build().await
    }
    async fn new_passkey() -> Tester<PasskeyUser, Arc<FfiXmtpClient>> {
        TesterBuilder::new().passkey_owner().await.build().await
    }

    fn builder() -> TesterBuilder<LocalWallet> {
        TesterBuilder::new()
    }
}

async fn create_raw_client<Owner>(builder: &TesterBuilder<Owner>) -> Arc<FfiXmtpClient>
where
    Owner: InboxOwner,
{
    let nonce = 1;
    let ident = builder.owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let client = create_client(
        connect_to_backend(xmtp_api_grpc::LOCALHOST_ADDRESS.to_string(), false)
            .await
            .unwrap(),
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::generate_enc_key().into()),
        &inbox_id,
        ident.into(),
        1,
        None,
        builder.sync_url.clone(),
        Some(builder.sync_mode.into()),
    )
    .await
    .unwrap();
    let conn = client.inner_client.context().store().conn().unwrap();
    conn.register_triggers();

    client
}

#![allow(clippy::unwrap_used)]
use std::sync::Arc;

use alloy::signers::local::PrivateKeySigner;
use xmtp_common::{TestLogReplace, tmp_path};
use xmtp_configuration::GrpcUrls;
use xmtp_id::InboxOwner;
use xmtp_mls::utils::{PasskeyUser, Tester, TesterBuilder};

use crate::inbox_owner::FfiInboxOwner;

use super::{inbox_owner::FfiWalletInboxOwner, *};

#[allow(async_fn_in_trait)]
pub trait LocalBuilder<Owner>
where
    Owner: InboxOwner + Clone,
{
    async fn build(&self) -> Tester<Owner, FfiXmtpClient>;
    async fn build_no_panic(&self) -> Result<Tester<Owner, FfiXmtpClient>, GenericError>;
}

impl LocalBuilder<PrivateKeySigner> for TesterBuilder<PrivateKeySigner> {
    async fn build(&self) -> Tester<PrivateKeySigner, FfiXmtpClient> {
        self.build_no_panic().await.unwrap()
    }

    // Will not panic on registering identity. Will still panic on just about everything else.
    async fn build_no_panic(
        &self,
    ) -> Result<Tester<PrivateKeySigner, FfiXmtpClient>, GenericError> {
        let client = create_raw_client(self).await;
        let mut replace = TestLogReplace::default();
        if let Some(name) = &self.name {
            let ident = self.owner.get_identifier().unwrap();
            replace.add(&ident.to_string(), &format!("{name}_ident"));
            replace.add(
                &client.inner_client.installation_public_key().to_string(),
                &format!("{name}_installation"),
            );
            replace.add(client.inner_client.inbox_id(), name);
        }
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

        let worker = client.inner_client.context.sync_metrics();

        if let Some(worker) = &worker
            && self.wait_for_init
        {
            worker.wait_for_init().await.unwrap();
        }

        Ok(Tester {
            builder: self.clone(),
            client,
            worker,
            stream_handle: None,
            replace,
            proxy: None,
        })
    }
}
impl LocalBuilder<PasskeyUser> for TesterBuilder<PasskeyUser> {
    async fn build(&self) -> Tester<PasskeyUser, FfiXmtpClient> {
        self.build_no_panic().await.unwrap()
    }

    async fn build_no_panic(&self) -> Result<Tester<PasskeyUser, FfiXmtpClient>, GenericError> {
        let client = create_raw_client(self).await;
        let mut replace = TestLogReplace::default();
        if let Some(name) = &self.name {
            let ident = self.owner.get_identifier().unwrap();
            replace.add(&ident.to_string(), &format!("{name}_ident"));
            replace.add(
                &client.inner_client.installation_public_key().to_string(),
                &format!("{name}_installation"),
            );
            replace.add(client.inner_client.inbox_id(), name);
        }

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

        let worker = client.inner_client.context.sync_metrics();

        if let Some(worker) = &worker
            && self.wait_for_init
        {
            worker.wait_for_init().await.unwrap();
        }

        Ok(Tester {
            builder: self.clone(),
            client,
            worker,
            stream_handle: None,
            replace,
            proxy: None,
        })
    }
}

#[allow(async_fn_in_trait)]
pub trait LocalTester {
    async fn new() -> Self;
    #[allow(unused)]
    async fn new_passkey() -> Tester<PasskeyUser, FfiXmtpClient>;
    fn builder() -> TesterBuilder<PrivateKeySigner>;
}
impl LocalTester for Tester<PrivateKeySigner, FfiXmtpClient> {
    async fn new() -> Self {
        TesterBuilder::new().build().await
    }
    async fn new_passkey() -> Tester<PasskeyUser, FfiXmtpClient> {
        TesterBuilder::new().passkey().build().await
    }
    fn builder() -> TesterBuilder<PrivateKeySigner> {
        TesterBuilder::new()
    }
}

pub async fn connect_to_backend_test() -> Arc<super::XmtpApiClient> {
    if cfg!(feature = "d14n") {
        connect_to_backend(
            GrpcUrls::NODE.to_string(),
            Some(GrpcUrls::GATEWAY.to_string()),
            false,
            None,
        )
        .await
        .unwrap()
    } else {
        connect_to_backend(GrpcUrls::NODE.to_string(), None, false, None)
            .await
            .unwrap()
    }
}

async fn create_raw_client<Owner>(builder: &TesterBuilder<Owner>) -> FfiXmtpClient
where
    Owner: InboxOwner,
{
    let nonce = 1;
    let ident = builder.owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let client = create_client(
        connect_to_backend_test().await,
        connect_to_backend_test().await,
        Some(tmp_path()),
        Some(xmtp_db::EncryptedMessageStore::<()>::generate_enc_key().into()),
        &inbox_id,
        ident.into(),
        1,
        None,
        builder.sync_url.clone(),
        Some(builder.sync_mode.into()),
        None,
        None,
    )
    .await
    .unwrap();
    let client = Arc::into_inner(client)
        .expect("Client was just created so no other strong references exist");
    let conn = client.inner_client.context.db();
    conn.register_triggers();

    client
}

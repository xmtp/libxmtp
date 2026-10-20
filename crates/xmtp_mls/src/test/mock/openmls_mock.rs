use super::*;
use crate::identity::XmtpKeyPackage;
use crate::identity::create_credential;
use openmls::prelude::{CredentialWithKey, KeyPackage};
use openmls::storage::OpenMlsProvider;
use openmls::test_utils::test_framework::client::Client;
use openmls::test_utils::test_framework::errors::ClientError;
use std::collections::HashMap;
use std::sync::RwLock;
use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_cryptography::configuration::CIPHERSUITE;
use xmtp_db::sql_key_store::SqlKeyStoreError;
use xmtp_db::xmtp_openmls_provider::XmtpOpenMlsProviderRef;
use xmtp_db::{MemoryStorage, MlsMemoryStorage, MlsProviderExt, XmtpOpenMlsProvider};
use xmtp_db::{
    XmtpMlsStorageProvider,
    sql_key_store::{SqlKeyStore, mock::MockSqlKeyStore},
};

pub struct BarebonesMlsClient<P: OpenMlsProvider> {
    installation_key: XmtpInstallationCredential,
    client: Client<P>,
}

pub trait OpenMlsTestExt {
    /// Builds a fresh KeyPackage and stores its reference in the local db
    fn key_package(&self) -> Result<KeyPackage, ClientError<SqlKeyStoreError>>;
}

/// create an owned anonymous client
pub fn gen_client(identity: &str) -> BarebonesMlsClient<XmtpOpenMlsProvider<MlsMemoryStorage>> {
    let store = SqlKeyStore::new(MemoryStorage::new());
    let mut credentials = HashMap::new();
    let installation_key = XmtpInstallationCredential::new();
    let key_pair = openmls_basic_credential::SignatureKeyPair::from(installation_key.clone());
    key_pair.store(&store).unwrap();
    let signature_key = installation_key.clone().into();
    let credential = CredentialWithKey {
        credential: create_credential(identity).unwrap(),
        signature_key,
    };
    credentials.insert(CIPHERSUITE, credential);
    let client = Client::<_> {
        identity: b"alice".to_vec(),
        credentials,
        provider: XmtpOpenMlsProvider::new(store),
        groups: RwLock::new(HashMap::new()),
    };

    BarebonesMlsClient {
        installation_key,
        client,
    }
}

pub fn create_mls_client<S: XmtpMlsStorageProvider>(
    store: &S,
) -> BarebonesMlsClient<XmtpOpenMlsProviderRef<'_, S>> {
    let mut credentials = HashMap::new();
    let installation_key = XmtpInstallationCredential::new();
    let key_pair = openmls_basic_credential::SignatureKeyPair::from(installation_key.clone());
    key_pair.store(store).unwrap();
    let signature_key = installation_key.clone().into();
    let credential = CredentialWithKey {
        credential: create_credential("alice").unwrap(),
        signature_key,
    };
    credentials.insert(CIPHERSUITE, credential);
    BarebonesMlsClient {
        installation_key,
        client: Client::<_> {
            identity: b"alice".to_vec(),
            credentials,
            provider: XmtpOpenMlsProviderRef::new(store),
            groups: RwLock::new(HashMap::new()),
        },
    }
}

impl MockStoreAndContext {
    /// Create an MLS client with an XMTP Installation Key
    /// Stores the Key package in OpenMls Memory storage
    /// Adds credential to client
    pub fn mls_client(&self) -> BarebonesMlsClient<XmtpOpenMlsProviderRef<'_, MockSqlKeyStore>> {
        create_mls_client(&self.mls_storage)
    }
}

impl<P: MlsProviderExt> OpenMlsTestExt for BarebonesMlsClient<P> {
    fn key_package(&self) -> Result<KeyPackage, ClientError<SqlKeyStoreError>> {
        let cred = self.client.credentials.get(&CIPHERSUITE).unwrap();
        let cred = &cred.credential;
        Ok(XmtpKeyPackage::builder()
            .inbox_id(String::from_utf8_lossy(&self.client.identity))
            .credential(cred.clone())
            .installation_keys(self.installation_key.clone())
            .build(&self.client.provider, false)
            .unwrap()
            .key_package)
    }
}

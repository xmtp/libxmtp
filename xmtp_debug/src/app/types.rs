use std::mem::size_of;
use std::sync::atomic::AtomicBool;

use alloy::signers::local::PrivateKeySigner;
use color_eyre::eyre::{self, Result};
use ecdsa::SigningKey;
use openmls::{credentials::BasicCredential, prelude::Credential};
use prost::Message;
use speedy::{Readable, Writable};

use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_id::associations::builder::SignatureRequest;
use xmtp_proto::xmtp::identity::MlsCredential;

/// An InboxId represented as fixed bytes
pub type InboxId = [u8; 32];

#[derive(Clone, Debug)]
pub struct EthereumWallet(SigningKey<k256::Secp256k1>);

impl EthereumWallet {
    pub fn into_alloy(self) -> PrivateKeySigner {
        #[allow(deprecated)]
        PrivateKeySigner::from_slice(self.0.to_bytes().as_slice()).expect("Should never fail")
    }

    fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes((&bytes).into()).unwrap())
    }

    // checksummed addresses for a bit more chaos
    fn address(&self) -> String {
        alloy::primitives::Address::to_checksum(&self.clone().into_alloy().address(), None)
    }
}

impl Default for EthereumWallet {
    fn default() -> Self {
        Self(SigningKey::random(&mut xmtp_cryptography::rand::rng()))
    }
}

impl<'a> From<&'a Identity> for EthereumWallet {
    fn from(identity: &'a Identity) -> EthereumWallet {
        EthereumWallet(
            SigningKey::<k256::Secp256k1>::from_bytes((&identity.eth_key).into()).unwrap(),
        )
    }
}

/// Identity specific to this debug CLI Tool.
/// An installation key and a eth address
#[derive(
    valuable::Valuable, Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Readable, Writable,
)]
pub struct Identity {
    pub inbox_id: [u8; 32],
    pub installation_key: [u8; 32],
    eth_key: [u8; 32],
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "inbox_id={},installation_key={},eth_key={}",
            hex::encode(self.inbox_id),
            hex::encode(self.installation_key),
            hex::encode(self.eth_key)
        )
    }
}

impl Identity {
    pub fn from_libxmtp(
        value: &xmtp_mls::identity::Identity,
        wallet: EthereumWallet,
    ) -> eyre::Result<Self> {
        let identity = unsafe {
            std::mem::transmute::<&xmtp_mls::identity::Identity, &ForeignIdentity>(value)
        };

        let mut inbox_id = [0u8; 32];
        let mut eth_key = [0u8; 32];
        hex::decode_to_slice(identity.inbox_id.clone(), &mut inbox_id)?;
        #[allow(deprecated)]
        eth_key.copy_from_slice(wallet.0.to_bytes().as_slice());
        Ok(Identity {
            inbox_id,
            installation_key: identity.installation_keys.private_bytes(),
            eth_key,
        })
    }

    pub fn address(&self) -> String {
        EthereumWallet::from_bytes(self.eth_key).address()
    }

    /// SQLite database path for this identity
    pub fn db_path(&self, network: impl Into<u64> + Copy) -> Result<std::path::PathBuf> {
        let dir = crate::app::App::db_directory(network)?;
        let db_name = format!("{}:{}.db3", hex::encode(self.inbox_id), network.into());
        Ok(dir.join(db_name))
    }
}

//TODO: Remove this unsafe transmutation by adding helpers to xmtp_mls
// its OK for now
impl From<Identity> for xmtp_mls::identity::Identity {
    fn from(value: Identity) -> Self {
        let inbox_id = hex::encode(value.inbox_id);
        let installation_keys =
            XmtpInstallationCredential::from_bytes(&value.installation_key).unwrap();
        let credential: Credential = BasicCredential::new(
            MlsCredential {
                inbox_id: inbox_id.clone(),
            }
            .encode_to_vec(),
        )
        .into();
        let identity = ForeignIdentity {
            inbox_id,
            installation_keys,
            credential,
            signature_request: None,
            is_ready: Default::default(),
        };
        unsafe { std::mem::transmute::<ForeignIdentity, xmtp_mls::identity::Identity>(identity) }
    }
}

#[allow(unused)]
struct ForeignIdentity {
    inbox_id: String,
    installation_keys: XmtpInstallationCredential,
    credential: Credential,
    signature_request: Option<SignatureRequest>,
    is_ready: AtomicBool,
}

impl redb::Value for Identity {
    type SelfType<'a>
        = Identity
    where
        Self: 'a;

    type AsBytes<'a>
        = [u8; size_of::<Identity>()]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(size_of::<Self::SelfType<'_>>())
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Identity::read_from_buffer(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let mut buffer = [0u8; size_of::<Identity>()];
        value.write_to_buffer(&mut buffer).unwrap();
        buffer
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("identity")
    }
}

/// Group specific to this debug CLI Tool.
/// Number of members in group
#[derive(Debug, Hash, PartialEq, Eq, valuable::Valuable, Readable, Writable)]
pub struct Group {
    /// user that created group
    pub created_by: InboxId,
    /// Id of the group
    pub id: [u8; 16],
    /// Size of the groups
    pub member_size: u32,
    /// members by inbox id
    pub members: Vec<InboxId>,
}

impl redb::Value for Group {
    type SelfType<'a>
        = Group
    where
        Self: 'a;

    type AsBytes<'a>
        = Vec<u8>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Group::read_from_buffer(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        value.write_to_vec().unwrap()
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("group")
    }
}

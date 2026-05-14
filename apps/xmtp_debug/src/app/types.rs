use std::mem::size_of;
use std::sync::atomic::AtomicBool;

use alloy::signers::local::PrivateKeySigner;
use color_eyre::eyre::{self, Result};
use ecdsa::SigningKey;
use openmls::{credentials::BasicCredential, prelude::Credential};
use prost::Message as _;
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
        Self::from_bytes(xmtp_cryptography::rand::rand_array())
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
#[derive(Debug, Hash, Clone, PartialEq, Eq, valuable::Valuable, Readable, Writable)]
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

impl std::fmt::Display for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.id))
    }
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

/// Message recorded for a single healthcheck `SendMessage` op.
/// Persisted to redb so the `NoMissingMessages` validator has an
/// authoritative source of truth across runs and versions.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Readable, Writable)]
pub struct Message {
    /// MLS message id (sha-256-derived hash from libxmtp's
    /// `calculate_message_id`). Always 32 bytes — runtime-asserted at
    /// the call site of `record_message`.
    pub id: [u8; 32],
    /// Group this message belongs to.
    pub group_id: [u8; 16],
    /// Sender's inbox_id (32-byte form, same convention as `Identity`).
    pub sender_inbox_id: InboxId,
    /// Wall-clock at the sending op's call site. libxmtp doesn't surface
    /// its internal envelope timestamp at `send_message` return, so this
    /// is best-effort for diagnostics. Not used by the validator.
    pub sent_at_ns: i64,
    /// UUID of the healthcheck run that sent this message.
    pub op_run_id: [u8; 16],
    /// `crate::get_version()` output of the sending xdbg binary.
    pub xdbg_version: String,
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.id))
    }
}

impl redb::Value for Message {
    type SelfType<'a>
        = Message
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
        Message::read_from_buffer(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        value.write_to_vec().unwrap()
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("message")
    }
}

#[cfg(test)]
mod message_tests {
    use super::*;

    #[test]
    fn message_speedy_roundtrip() {
        let msg = Message {
            id: [7u8; 32],
            group_id: [3u8; 16],
            sender_inbox_id: [9u8; 32],
            sent_at_ns: 1_700_000_000_000_000_000,
            op_run_id: [42u8; 16],
            xdbg_version: "1.10.0-abcdefg".to_string(),
        };
        let bytes = msg.write_to_vec().unwrap();
        let decoded = Message::read_from_buffer(&bytes).unwrap();
        assert_eq!(msg, decoded);
    }

    use crate::app::store::DeriveKey;
    use crate::app::store::MessageKey;

    #[test]
    fn message_derive_key_composes_group_and_message_id() {
        let msg = Message {
            id: [0xAAu8; 32],
            group_id: [0xBBu8; 16],
            sender_inbox_id: [0u8; 32],
            sent_at_ns: 0,
            op_run_id: [0u8; 16],
            xdbg_version: String::new(),
        };
        let key: MessageKey = msg.key(7);
        // u64 network (8 bytes, LE) + 48-byte combined key.
        let bytes = speedy::Writable::write_to_vec(&key).unwrap();
        assert_eq!(bytes.len(), 8 + 48);
        assert_eq!(&bytes[0..8], &7u64.to_le_bytes());
        assert_eq!(&bytes[8..24], &[0xBBu8; 16]);
        assert_eq!(&bytes[24..56], &[0xAAu8; 32]);
    }
}

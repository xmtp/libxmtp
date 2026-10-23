use std::mem::size_of;

use alloy_signer_local::PrivateKeySigner;
use color_eyre::eyre::{self, Result};
use ecdsa::SigningKey;
use openmls::{credentials::BasicCredential, prelude::Credential};
use prost::Message as _;
use speedy::{Readable, Writable};

use xmtp_cryptography::XmtpInstallationCredential;
use xmtp_id::{InboxOwner, associations::builder::SignatureRequest};
use xmtp_mls_common::group_metadata::DmMembers;
use xmtp_proto::{types::GroupId, xmtp::identity::MlsCredential};

use crate::{
    XDBG_ID_NONCE,
    app::{
        App,
        store::{MessageKey, message_key},
    },
    args,
};

/// An InboxId represented as fixed bytes
pub type InboxId = [u8; 32];

#[derive(Clone, Debug)]
pub struct EthereumWallet(SigningKey<k256::Secp256k1>);

impl EthereumWallet {
    pub fn into_alloy(self) -> PrivateKeySigner {
        PrivateKeySigner::from_signing_key(self.0)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SigningKey::from_bytes((&bytes).into()).unwrap())
    }

    // checksummed addresses for a bit more chaos
    fn address(&self) -> String {
        alloy_primitives::Address::to_checksum(&self.clone().into_alloy().address(), None)
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

/// Max bytes for the recorded `version_string` field. 64 fits
/// `<cargo-pkg-version>-<vergen-output>` with room to spare; chosen so
/// `Identity` stays a fixed-width redb value.
pub const VERSION_STRING_LEN: usize = 64;

/// Right-zero-pad a version string into the fixed-width buffer.
/// Errors if the string exceeds the buffer size (rather than
/// silently truncating — a too-long version means the cap is
/// wrong and we should bump VERSION_STRING_LEN).
fn pack_version_string(s: &str) -> Result<[u8; VERSION_STRING_LEN]> {
    let bytes = s.as_bytes();
    if bytes.len() > VERSION_STRING_LEN {
        return Err(eyre::eyre!(
            "version string '{s}' is {} bytes; exceeds VERSION_STRING_LEN={VERSION_STRING_LEN}",
            bytes.len()
        ));
    }
    let mut out = [0u8; VERSION_STRING_LEN];
    out[..bytes.len()].copy_from_slice(bytes);
    Ok(out)
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
    pub version_hash: u64,
    /// `crate::get_version()` output of the xdbg binary that created
    /// this identity. Right-zero-padded UTF-8. Use
    /// `Identity::version_string` to decode.
    pub version_string: [u8; VERSION_STRING_LEN],
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
    pub fn generate() -> Result<Self> {
        let wallet = EthereumWallet::default();
        let local = wallet.clone().into_alloy();
        let ident = local.get_identifier()?;
        let inbox_id = ident.inbox_id(XDBG_ID_NONCE)?;
        let cred = XmtpInstallationCredential::new();
        let mut eth_key = [0u8; 32];
        let mut inbox_id_bytes = [0u8; 32];
        // zeroize: none, xdbg persists keys in plaintext by design
        #[allow(deprecated, clippy::disallowed_methods)]
        eth_key.copy_from_slice(wallet.0.to_bytes().as_slice());
        hex::decode_to_slice(inbox_id.clone(), &mut inbox_id_bytes)?;
        Ok(Identity {
            inbox_id: inbox_id_bytes,
            installation_key: *cred.private_bytes(),
            eth_key,
            version_hash: crate::app::App::current_version_hash(),
            version_string: pack_version_string(&crate::get_version())?,
        })
    }

    pub fn from_libxmtp(
        value: &xmtp_mls::identity::Identity,
        wallet: EthereumWallet,
    ) -> eyre::Result<Self> {
        let mut inbox_id = [0u8; 32];
        let mut eth_key = [0u8; 32];
        hex::decode_to_slice(value.inbox_id(), &mut inbox_id)?;
        // zeroize: none, xdbg persists keys in plaintext by design
        #[allow(deprecated, clippy::disallowed_methods)]
        eth_key.copy_from_slice(wallet.0.to_bytes().as_slice());
        Ok(Identity {
            inbox_id,
            installation_key: *value.installation_keys().private_bytes(),
            eth_key,
            version_hash: crate::app::App::current_version_hash(),
            version_string: pack_version_string(&crate::get_version())?,
        })
    }

    /// Decode the recorded version string (UTF-8 with trailing NUL
    /// padding stripped). Errors if the bytes aren't valid UTF-8.
    pub fn version_string(&self) -> Result<&str> {
        let end = self
            .version_string
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(VERSION_STRING_LEN);
        std::str::from_utf8(&self.version_string[..end])
            .map_err(|e| eyre::eyre!("invalid utf-8 in version_string: {e}"))
    }

    pub fn address(&self) -> String {
        EthereumWallet::from_bytes(self.eth_key).address()
    }

    /// SQLite database path for this identity
    pub fn db_path(&self, network: impl Into<u64> + Copy) -> Result<std::path::PathBuf> {
        let dir = crate::app::App::db_directory()?;
        let db_name = format!("{}:{}.db3", hex::encode(self.inbox_id), network.into());
        Ok(dir.join(db_name))
    }

    /// SQLite path for this identity, addressing a specific
    /// `version_hash` partition. Used when loading an identity that
    /// was written by a different xdbg version (non-strict mode).
    pub fn db_path_for(&self, version_hash: u64) -> Result<std::path::PathBuf> {
        let dir = crate::app::App::db_directory_for(version_hash)?;
        let net_hash = App::network_hash();
        let db_name = format!("{}:{net_hash}.db3", hex::encode(self.inbox_id));
        Ok(dir.join(db_name))
    }

    #[cfg(test)]
    pub fn dummy_for_test() -> Identity {
        Identity {
            inbox_id: [0u8; 32],
            installation_key: [0u8; 32],
            eth_key: [0u8; 32],
            version_hash: 0,
            version_string: [0u8; VERSION_STRING_LEN],
        }
    }

    pub fn inbox_id(&self) -> args::InboxId {
        args::InboxId::new(self.inbox_id)
    }
}

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
        
        // Uses the safe public constructor instead of unsafe transmute
        xmtp_mls::identity::Identity::from_parts(
            inbox_id,
            installation_keys,
            credential,
            None,
            Default::default(),
        )
    }
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

/// The 32-byte group id xdbg stores. Every group a client creates or joins is
/// keyed, so its id is the 32-byte public key of its group signing key. A
/// 16-byte legacy id comes only from an imported archive or a command
/// argument, and is an error, not a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredGroupId(pub [u8; 32]);

impl From<[u8; 32]> for StoredGroupId {
    fn from(value: [u8; 32]) -> Self {
        Self(value)
    }
}

impl TryFrom<&GroupId> for StoredGroupId {
    type Error = eyre::Report;

    fn try_from(value: &GroupId) -> Result<Self> {
        value.as_slice().try_into().map(Self).map_err(|_| {
            eyre::eyre!(
                "xdbg stores only 32-byte group ids; {value} has {} bytes",
                value.len()
            )
        })
    }
}

/// Group specific to this debug CLI Tool.
/// Number of members in group
#[derive(Debug, Hash, Clone, PartialEq, Eq, valuable::Valuable, Readable, Writable)]
pub struct Group {
    /// user that created group
    pub created_by: InboxId,
    /// Id of the group. Stored as `[u8; 32]` so it can be serialized via
    /// `speedy`; use [`Group::id`] when interacting with xmtp_mls
    /// APIs that expect a `GroupId`.
    id: [u8; 32],
    /// Size of the groups
    pub member_size: u32,
    /// members by inbox id
    pub members: Vec<InboxId>,
    /// `crate::get_version()` of the xdbg binary that created this
    /// group, right-zero-padded UTF-8. Same shape as
    /// `Identity::version_string`. Decode via `Group::version_string`.
    pub version_string: [u8; VERSION_STRING_LEN],
}

impl Group {
    /// Build a `Group` from id + creator + members; the redundant
    /// `member_size` field is derived. Use this everywhere instead of
    /// the struct literal so the size never drifts from `members.len()`.
    pub fn new(id: impl Into<StoredGroupId>, created_by: InboxId, members: Vec<InboxId>) -> Self {
        let member_size = members.len() as u32;
        Self {
            created_by,
            id: id.into().0,
            member_size,
            members,
            version_string: pack_version_string(&crate::get_version())
                .expect("version must be valid"),
        }
    }

    /// View `self.id` as the canonical [`xmtp_proto::types::GroupId`].
    pub fn id(&self) -> GroupId {
        GroupId::from(self.id)
    }

    /// The stored 32-byte id.
    pub fn stored_id(&self) -> StoredGroupId {
        StoredGroupId(self.id)
    }

    /// Merge `new` inboxes into the existing members, deduping.
    /// `member_size` is re-derived from the merged length.
    pub fn add_members(self, new: impl IntoIterator<Item = InboxId>) -> Self {
        let Group {
            id,
            created_by,
            mut members,
            ..
        } = self;
        let mut seen: std::collections::HashSet<InboxId> = members.iter().copied().collect();
        for inbox in new {
            if seen.insert(inbox) {
                members.push(inbox);
            }
        }
        Group::new(id, created_by, members)
    }

    pub fn has_member(&self, inbox: &InboxId) -> bool {
        self.members.contains(inbox)
    }
}

impl std::fmt::Display for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
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

#[derive(Debug, Clone, Hash, PartialEq, Eq, Readable, Writable)]
pub struct Dm {
    /// user that created dm
    pub created_by: InboxId,

    with: InboxId,

    group_id: [u8; 32],

    /// `crate::get_version()` of the xdbg binary that created this
    /// group, right-zero-padded UTF-8. Same shape as
    /// `Identity::version_string`. Decode via `Group::version_string`.
    pub version_string: [u8; VERSION_STRING_LEN],
}

pub struct DmId([u8; 64]);
impl DmId {
    /// Build a sorted-pair DM key from two 32-byte inbox ids. Sorting
    /// ensures `DmId::new(a, b) == DmId::new(b, a)` so creator order
    /// doesn't fragment the keyspace.
    pub fn new(lhs: [u8; 32], rhs: [u8; 32]) -> Self {
        let mut pair = [lhs, rhs];
        pair.sort();
        let mut id = [0u8; 64];
        id[..32].copy_from_slice(&pair[0]);
        id[32..].copy_from_slice(&pair[1]);
        Self(id)
    }

    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    pub fn into_bytes(self) -> [u8; 64] {
        self.0
    }
}

impl std::fmt::Display for Dm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id())
    }
}

impl Dm {
    pub fn new(created_by: InboxId, with: InboxId, group_id: StoredGroupId) -> Self {
        Self {
            created_by,
            with,
            group_id: group_id.0,
            version_string: pack_version_string(&crate::get_version())
                .expect("version must be valid"),
        }
    }

    /// return the XMTP DM ID (dm:inbox1:inbox2)
    pub fn id(&self) -> String {
        DmMembers {
            member_one_inbox_id: hex::encode(self.created_by),
            member_two_inbox_id: hex::encode(self.with),
        }
        .to_string()
    }

    pub fn redb_key(&self) -> DmId {
        DmId::new(self.created_by, self.with)
    }
}

impl redb::Value for Dm {
    type SelfType<'a>
        = Dm
    where
        Self: 'a;

    type AsBytes<'a>
        = [u8; size_of::<Dm>()]
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        Some(size_of::<Self::SelfType<'_>>())
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Dm::read_from_buffer(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        let mut buffer = [0u8; size_of::<Dm>()];
        value.write_to_buffer(&mut buffer).unwrap();
        buffer
    }

    fn type_name() -> redb::TypeName {
        redb::TypeName::new("dm")
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
    group_id: [u8; 32],
    /// Sender's inbox_id (32-byte form, same convention as `Identity`).
    pub sender_inbox_id: InboxId,
    /// Wall-clock at the sending op's call site. libxmtp doesn't surface
    /// its internal envelope timestamp at `send_message` return, so this
    /// is best-effort for diagnostics. Not used by the validator.
    pub sent_at_ns: i64,
    /// `crate::get_version()` output of the sending xdbg binary.
    pub xdbg_version: String,
}
impl Message {
    pub fn new(
        id: [u8; 32],
        group_id: impl Into<StoredGroupId>,
        sender_inbox_id: InboxId,
        sent_at_ns: i64,
    ) -> Self {
        Message {
            id,
            group_id: group_id.into().0,
            sender_inbox_id,
            sent_at_ns,
            xdbg_version: crate::get_version(),
        }
    }

    /// Compute the redb `MessageKey` for `(network, group_id, id)`
    /// without constructing a full `Message`. The on-disk key only
    /// uses `group_id ++ id` (64 bytes); the rest of the message
    /// is the value, not the key. Use this for membership checks
    /// to avoid allocating a placeholder `Message`.
    pub fn redb_key(group_id: impl Into<StoredGroupId>, id: [u8; 32]) -> MessageKey {
        message_key(group_id.into().0, id)
    }

    pub fn group_id(&self) -> GroupId {
        GroupId::from(self.group_id)
    }

    /// The stored 32-byte id of the group this message belongs to.
    pub fn stored_group_id(&self) -> StoredGroupId {
        StoredGroupId(self.group_id)
    }
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
mod stored_group_id_tests {
    use super::*;
    use crate::app::store::GroupKey;

    /// Every group a client creates is keyed, so xdbg stores its 32-byte id.
    /// A 16-byte id, which only an imported archive or a command argument can
    /// carry, is refused with an error, not a panic.
    #[xmtp_common::test(unwrap_try = true)]
    fn a_keyed_group_id_is_stored_and_a_legacy_one_refused() {
        let keyed = GroupId::from([7u8; 32]);
        let stored = StoredGroupId::try_from(&keyed)?;
        assert_eq!(Group::new(stored, [1; 32], vec![[1; 32]]).id(), keyed);
        assert!(GroupKey::try_from(&keyed).is_ok());
        let legacy = GroupId::from([7u8; 16]);
        assert!(StoredGroupId::try_from(&legacy).is_err());
        assert!(GroupKey::try_from(&legacy).is_err());
    }
}

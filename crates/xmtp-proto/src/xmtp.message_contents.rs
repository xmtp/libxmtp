// @generated
/// Signature represents a generalized public key signature,
/// defined as a union to support cryptographic algorithm agility.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Signature {
    #[prost(oneof="signature::Union", tags="1, 2")]
    pub union: ::core::option::Option<signature::Union>,
}
/// Nested message and enum types in `Signature`.
pub mod signature {
    /// ECDSA signature bytes and the recovery bit
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct EcdsaCompact {
        /// compact representation [ R || S ], 64 bytes
        #[prost(bytes="vec", tag="1")]
        pub bytes: ::prost::alloc::vec::Vec<u8>,
        /// recovery bit
        #[prost(uint32, tag="2")]
        pub recovery: u32,
    }
    /// ECDSA signature bytes and the recovery bit
    /// produced by xmtp-js::PublicKey.signWithWallet function, i.e.
    /// EIP-191 signature of a "Create Identity" message with the key embedded.
    /// Used to sign identity keys.
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct WalletEcdsaCompact {
        /// compact representation [ R || S ], 64 bytes
        #[prost(bytes="vec", tag="1")]
        pub bytes: ::prost::alloc::vec::Vec<u8>,
        /// recovery bit
        #[prost(uint32, tag="2")]
        pub recovery: u32,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Union {
        #[prost(message, tag="1")]
        EcdsaCompact(EcdsaCompact),
        #[prost(message, tag="2")]
        WalletEcdsaCompact(WalletEcdsaCompact),
    }
}
/// Ciphertext represents encrypted payload.
/// It is definited as a union to support cryptographic algorithm agility.
/// The payload is accompanied by the cryptographic parameters
/// required by the chosen encryption scheme.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ciphertext {
    #[prost(oneof="ciphertext::Union", tags="1")]
    pub union: ::core::option::Option<ciphertext::Union>,
}
/// Nested message and enum types in `Ciphertext`.
pub mod ciphertext {
    // Supported Encryption Schemes

    /// Encryption: AES256-GCM
    /// Key derivation function: HKDF-SHA256
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Aes256gcmHkdfsha256 {
        /// 32 bytes
        #[prost(bytes="vec", tag="1")]
        pub hkdf_salt: ::prost::alloc::vec::Vec<u8>,
        /// 12 bytes
        #[prost(bytes="vec", tag="2")]
        pub gcm_nonce: ::prost::alloc::vec::Vec<u8>,
        /// encrypted payload
        #[prost(bytes="vec", tag="3")]
        pub payload: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Union {
        #[prost(message, tag="1")]
        Aes256GcmHkdfSha256(Aes256gcmHkdfsha256),
    }
}
/// SignedEciesCiphertext represents an ECIES encrypted payload and a signature
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedEciesCiphertext {
    /// serialized Ecies message
    #[prost(bytes="vec", tag="1")]
    pub ecies_bytes: ::prost::alloc::vec::Vec<u8>,
    /// signature of sha256(ecies_bytes) signed with the IdentityKey
    #[prost(message, optional, tag="2")]
    pub signature: ::core::option::Option<Signature>,
}
/// Nested message and enum types in `SignedEciesCiphertext`.
pub mod signed_ecies_ciphertext {
    /// Ecies is ciphertext encrypted using ECIES with a MAC
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Ecies {
        /// 65 bytes
        #[prost(bytes="vec", tag="1")]
        pub ephemeral_public_key: ::prost::alloc::vec::Vec<u8>,
        /// 16 bytes
        #[prost(bytes="vec", tag="2")]
        pub iv: ::prost::alloc::vec::Vec<u8>,
        /// 32 bytes
        #[prost(bytes="vec", tag="3")]
        pub mac: ::prost::alloc::vec::Vec<u8>,
        /// encrypted payload with block size of 16
        #[prost(bytes="vec", tag="4")]
        pub ciphertext: ::prost::alloc::vec::Vec<u8>,
    }
}
/// UnsignedPublicKey represents a generalized public key,
/// defined as a union to support cryptographic algorithm agility.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnsignedPublicKey {
    #[prost(uint64, tag="1")]
    pub created_ns: u64,
    #[prost(oneof="unsigned_public_key::Union", tags="3")]
    pub union: ::core::option::Option<unsigned_public_key::Union>,
}
/// Nested message and enum types in `UnsignedPublicKey`.
pub mod unsigned_public_key {
    // Supported key types

    /// EC: SECP256k1
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Secp256k1Uncompressed {
        /// uncompressed point with prefix (0x04) [ P || X || Y ], 65 bytes
        #[prost(bytes="vec", tag="1")]
        pub bytes: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Union {
        #[prost(message, tag="3")]
        Secp256k1Uncompressed(Secp256k1Uncompressed),
    }
}
/// SignedPublicKey 
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedPublicKey {
    /// embeds an UnsignedPublicKey
    #[prost(bytes="vec", tag="1")]
    pub key_bytes: ::prost::alloc::vec::Vec<u8>,
    /// signs key_bytes
    #[prost(message, optional, tag="2")]
    pub signature: ::core::option::Option<Signature>,
}
/// PublicKeyBundle packages the cryptographic keys associated with a wallet.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedPublicKeyBundle {
    /// Identity key MUST be signed by the wallet.
    #[prost(message, optional, tag="1")]
    pub identity_key: ::core::option::Option<SignedPublicKey>,
    /// Pre-key MUST be signed by the identity key.
    #[prost(message, optional, tag="2")]
    pub pre_key: ::core::option::Option<SignedPublicKey>,
}
// LEGACY

/// PublicKey represents a generalized public key,
/// defined as a union to support cryptographic algorithm agility.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublicKey {
    #[prost(uint64, tag="1")]
    pub timestamp: u64,
    #[prost(message, optional, tag="2")]
    pub signature: ::core::option::Option<Signature>,
    #[prost(oneof="public_key::Union", tags="3")]
    pub union: ::core::option::Option<public_key::Union>,
}
/// Nested message and enum types in `PublicKey`.
pub mod public_key {
    /// The key bytes
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Secp256k1Uncompressed {
        /// uncompressed point with prefix (0x04) [ P || X || Y ], 65 bytes
        #[prost(bytes="vec", tag="1")]
        pub bytes: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Union {
        #[prost(message, tag="3")]
        Secp256k1Uncompressed(Secp256k1Uncompressed),
    }
}
/// PublicKeyBundle packages the cryptographic keys associated with a wallet,
/// both senders and recipients are identified by their key bundles.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublicKeyBundle {
    /// Identity key MUST be signed by the wallet.
    #[prost(message, optional, tag="1")]
    pub identity_key: ::core::option::Option<PublicKey>,
    /// Pre-key MUST be signed by the identity key.
    #[prost(message, optional, tag="2")]
    pub pre_key: ::core::option::Option<PublicKey>,
}
/// Unsealed invitation V1
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InvitationV1 {
    /// topic name chosen for this conversation.
    /// It MUST be randomly generated bytes (length >= 32),
    /// then base64 encoded without padding
    #[prost(string, tag="1")]
    pub topic: ::prost::alloc::string::String,
    /// A context object defining metadata
    #[prost(message, optional, tag="2")]
    pub context: ::core::option::Option<invitation_v1::Context>,
    /// message encryption scheme and keys for this conversation.
    #[prost(oneof="invitation_v1::Encryption", tags="3")]
    pub encryption: ::core::option::Option<invitation_v1::Encryption>,
}
/// Nested message and enum types in `InvitationV1`.
pub mod invitation_v1 {
    /// Supported encryption schemes
    /// AES256-GCM-HKDF-SHA256
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Aes256gcmHkdfsha256 {
        /// randomly generated key material (32 bytes)
        #[prost(bytes="vec", tag="1")]
        pub key_material: ::prost::alloc::vec::Vec<u8>,
    }
    /// The context type
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Context {
        /// Expected to be a URI (ie xmtp.org/convo1)
        #[prost(string, tag="1")]
        pub conversation_id: ::prost::alloc::string::String,
        /// Key value map of additional metadata that would be exposed to
        /// application developers and could be used for filtering
        #[prost(map="string, string", tag="2")]
        pub metadata: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    }
    /// message encryption scheme and keys for this conversation.
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Encryption {
        /// Specify the encryption method to process the key material properly.
        #[prost(message, tag="3")]
        Aes256GcmHkdfSha256(Aes256gcmHkdfsha256),
    }
}
/// Sealed Invitation V1 Header
/// Header carries information that is unencrypted, thus readable by the network
/// it is however authenticated as associated data with the AEAD scheme used
/// to encrypt the invitation body, thus providing tamper evidence.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SealedInvitationHeaderV1 {
    #[prost(message, optional, tag="1")]
    pub sender: ::core::option::Option<SignedPublicKeyBundle>,
    #[prost(message, optional, tag="2")]
    pub recipient: ::core::option::Option<SignedPublicKeyBundle>,
    #[prost(uint64, tag="3")]
    pub created_ns: u64,
}
/// Sealed Invitation V1
/// Invitation encrypted with key material derived from the sender's and
/// recipient's public key bundles using simplified X3DH where
/// the sender's ephemeral key is replaced with sender's pre-key.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SealedInvitationV1 {
    /// encoded SealedInvitationHeaderV1 used as associated data for Ciphertext
    #[prost(bytes="vec", tag="1")]
    pub header_bytes: ::prost::alloc::vec::Vec<u8>,
    /// Ciphertext.payload MUST contain encrypted InvitationV1.
    #[prost(message, optional, tag="2")]
    pub ciphertext: ::core::option::Option<Ciphertext>,
}
/// Versioned Sealed Invitation
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SealedInvitation {
    #[prost(oneof="sealed_invitation::Version", tags="1")]
    pub version: ::core::option::Option<sealed_invitation::Version>,
}
/// Nested message and enum types in `SealedInvitation`.
pub mod sealed_invitation {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Version {
        #[prost(message, tag="1")]
        V1(super::SealedInvitationV1),
    }
}
/// A light pointer for a conversation that contains no decryption keys
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConversationReference {
    #[prost(string, tag="1")]
    pub topic: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub peer_address: ::prost::alloc::string::String,
    #[prost(uint64, tag="3")]
    pub created_ns: u64,
    #[prost(message, optional, tag="4")]
    pub context: ::core::option::Option<invitation_v1::Context>,
}
/// ContentTypeId is used to identify the type of content stored in a Message.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContentTypeId {
    /// authority governing this content type
    #[prost(string, tag="1")]
    pub authority_id: ::prost::alloc::string::String,
    /// type identifier
    #[prost(string, tag="2")]
    pub type_id: ::prost::alloc::string::String,
    /// major version of the type
    #[prost(uint32, tag="3")]
    pub version_major: u32,
    /// minor version of the type
    #[prost(uint32, tag="4")]
    pub version_minor: u32,
}
/// EncodedContent bundles the content with metadata identifying its type
/// and parameters required for correct decoding and presentation of the content.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EncodedContent {
    /// content type identifier used to match the payload with
    /// the correct decoding machinery
    #[prost(message, optional, tag="1")]
    pub r#type: ::core::option::Option<ContentTypeId>,
    /// optional encoding parameters required to correctly decode the content
    #[prost(map="string, string", tag="2")]
    pub parameters: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    /// optional fallback description of the content that can be used in case
    /// the client cannot decode or render the content
    #[prost(string, optional, tag="3")]
    pub fallback: ::core::option::Option<::prost::alloc::string::String>,
    /// optional compression; the value indicates algorithm used to
    /// compress the encoded content bytes
    #[prost(enumeration="Compression", optional, tag="5")]
    pub compression: ::core::option::Option<i32>,
    /// encoded content itself
    #[prost(bytes="vec", tag="4")]
    pub content: ::prost::alloc::vec::Vec<u8>,
}
/// SignedContent attaches a signature to EncodedContent.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedContent {
    /// MUST contain EncodedContent
    #[prost(bytes="vec", tag="1")]
    pub payload: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag="2")]
    pub sender: ::core::option::Option<SignedPublicKeyBundle>,
    /// MUST be a signature of a concatenation of
    /// the message header bytes and the payload bytes,
    /// signed by the sender's pre-key.
    #[prost(message, optional, tag="3")]
    pub signature: ::core::option::Option<Signature>,
}
/// Recognized compression algorithms
/// protolint:disable ENUM_FIELD_NAMES_ZERO_VALUE_END_WITH
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Compression {
    Deflate = 0,
    Gzip = 1,
}
impl Compression {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Compression::Deflate => "COMPRESSION_DEFLATE",
            Compression::Gzip => "COMPRESSION_GZIP",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "COMPRESSION_DEFLATE" => Some(Self::Deflate),
            "COMPRESSION_GZIP" => Some(Self::Gzip),
            _ => None,
        }
    }
}
/// Composite is used to implement xmtp.org/composite content type
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Composite {
    #[prost(message, repeated, tag="1")]
    pub parts: ::prost::alloc::vec::Vec<composite::Part>,
}
/// Nested message and enum types in `Composite`.
pub mod composite {
    /// Part represents one section of a composite message
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Part {
        #[prost(oneof="part::Element", tags="1, 2")]
        pub element: ::core::option::Option<part::Element>,
    }
    /// Nested message and enum types in `Part`.
    pub mod part {
        #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
        pub enum Element {
            #[prost(message, tag="1")]
            Part(super::super::EncodedContent),
            #[prost(message, tag="2")]
            Composite(super::super::Composite),
        }
    }
}
/// LEGACY: User key bundle V1 using PublicKeys.
/// The PublicKeys MUST be signed.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContactBundleV1 {
    #[prost(message, optional, tag="1")]
    pub key_bundle: ::core::option::Option<PublicKeyBundle>,
}
/// User key bundle V2 using SignedPublicKeys.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContactBundleV2 {
    #[prost(message, optional, tag="1")]
    pub key_bundle: ::core::option::Option<SignedPublicKeyBundle>,
}
/// Versioned ContactBundle
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContactBundle {
    #[prost(oneof="contact_bundle::Version", tags="1, 2")]
    pub version: ::core::option::Option<contact_bundle::Version>,
}
/// Nested message and enum types in `ContactBundle`.
pub mod contact_bundle {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Version {
        #[prost(message, tag="1")]
        V1(super::ContactBundleV1),
        #[prost(message, tag="2")]
        V2(super::ContactBundleV2),
    }
}
// Message V1

/// Message header is encoded separately as the bytes are also used
/// as associated data for authenticated encryption
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageHeaderV1 {
    #[prost(message, optional, tag="1")]
    pub sender: ::core::option::Option<PublicKeyBundle>,
    #[prost(message, optional, tag="2")]
    pub recipient: ::core::option::Option<PublicKeyBundle>,
    #[prost(uint64, tag="3")]
    pub timestamp: u64,
}
/// Message is the top level protocol element
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageV1 {
    /// encapsulates encoded MessageHeaderV1
    #[prost(bytes="vec", tag="1")]
    pub header_bytes: ::prost::alloc::vec::Vec<u8>,
    /// Ciphertext.payload MUST contain encrypted EncodedContent
    #[prost(message, optional, tag="2")]
    pub ciphertext: ::core::option::Option<Ciphertext>,
}
// Message V2

/// Message header carries information that is not encrypted, and is therefore
/// observable by the network. It is however authenticated as associated data
/// of the AEAD encryption used to protect the message,
/// thus providing tamper evidence.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageHeaderV2 {
    /// sender specified message creation time
    #[prost(uint64, tag="1")]
    pub created_ns: u64,
    /// the topic the message belongs to
    #[prost(string, tag="2")]
    pub topic: ::prost::alloc::string::String,
}
/// Message combines the encoded header with the encrypted payload.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageV2 {
    /// encapsulates encoded MessageHeaderV2
    #[prost(bytes="vec", tag="1")]
    pub header_bytes: ::prost::alloc::vec::Vec<u8>,
    /// Ciphertext.payload MUST contain encrypted SignedContent
    #[prost(message, optional, tag="2")]
    pub ciphertext: ::core::option::Option<Ciphertext>,
}
/// Versioned Message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(oneof="message::Version", tags="1, 2")]
    pub version: ::core::option::Option<message::Version>,
}
/// Nested message and enum types in `Message`.
pub mod message {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Version {
        #[prost(message, tag="1")]
        V1(super::MessageV1),
        #[prost(message, tag="2")]
        V2(super::MessageV2),
    }
}
/// DecodedMessage represents the decrypted message contents.
/// DecodedMessage instances are not stored on the network, but
/// may be serialized and stored by clients
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DecodedMessage {
    #[prost(string, tag="1")]
    pub id: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub message_version: ::prost::alloc::string::String,
    #[prost(string, tag="3")]
    pub sender_address: ::prost::alloc::string::String,
    #[prost(string, optional, tag="4")]
    pub recipient_address: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint64, tag="5")]
    pub sent_ns: u64,
    #[prost(string, tag="6")]
    pub content_topic: ::prost::alloc::string::String,
    #[prost(message, optional, tag="7")]
    pub conversation: ::core::option::Option<ConversationReference>,
    /// encapsulates EncodedContent
    #[prost(bytes="vec", tag="8")]
    pub content_bytes: ::prost::alloc::vec::Vec<u8>,
}
/// PrivateKey generalized to support different key types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SignedPrivateKey {
    /// time the key was created
    #[prost(uint64, tag="1")]
    pub created_ns: u64,
    /// public key for this private key
    #[prost(message, optional, tag="3")]
    pub public_key: ::core::option::Option<SignedPublicKey>,
    /// private key
    #[prost(oneof="signed_private_key::Union", tags="2")]
    pub union: ::core::option::Option<signed_private_key::Union>,
}
/// Nested message and enum types in `SignedPrivateKey`.
pub mod signed_private_key {
    // Supported key types

    /// EC: SECP256k1
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Secp256k1 {
        /// D big-endian, 32 bytes
        #[prost(bytes="vec", tag="1")]
        pub bytes: ::prost::alloc::vec::Vec<u8>,
    }
    /// private key
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Union {
        #[prost(message, tag="2")]
        Secp256k1(Secp256k1),
    }
}
/// PrivateKeyBundle wraps the identityKey and the preKeys,
/// enforces usage of signed keys.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PrivateKeyBundleV2 {
    #[prost(message, optional, tag="1")]
    pub identity_key: ::core::option::Option<SignedPrivateKey>,
    /// all the known pre-keys, newer keys first,
    #[prost(message, repeated, tag="2")]
    pub pre_keys: ::prost::alloc::vec::Vec<SignedPrivateKey>,
}
/// LEGACY: PrivateKey generalized to support different key types
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PrivateKey {
    /// time the key was created
    #[prost(uint64, tag="1")]
    pub timestamp: u64,
    /// public key for this private key
    #[prost(message, optional, tag="3")]
    pub public_key: ::core::option::Option<PublicKey>,
    /// private key
    #[prost(oneof="private_key::Union", tags="2")]
    pub union: ::core::option::Option<private_key::Union>,
}
/// Nested message and enum types in `PrivateKey`.
pub mod private_key {
    // Supported key types

    /// EC: SECP256k1
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Secp256k1 {
        /// D big-endian, 32 bytes
        #[prost(bytes="vec", tag="1")]
        pub bytes: ::prost::alloc::vec::Vec<u8>,
    }
    /// private key
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Union {
        #[prost(message, tag="2")]
        Secp256k1(Secp256k1),
    }
}
/// LEGACY: PrivateKeyBundleV1 wraps the identityKey and the preKeys
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PrivateKeyBundleV1 {
    #[prost(message, optional, tag="1")]
    pub identity_key: ::core::option::Option<PrivateKey>,
    /// all the known pre-keys, newer keys first,
    #[prost(message, repeated, tag="2")]
    pub pre_keys: ::prost::alloc::vec::Vec<PrivateKey>,
}
/// Versioned PrivateKeyBundle
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PrivateKeyBundle {
    #[prost(oneof="private_key_bundle::Version", tags="1, 2")]
    pub version: ::core::option::Option<private_key_bundle::Version>,
}
/// Nested message and enum types in `PrivateKeyBundle`.
pub mod private_key_bundle {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Version {
        #[prost(message, tag="1")]
        V1(super::PrivateKeyBundleV1),
        #[prost(message, tag="2")]
        V2(super::PrivateKeyBundleV2),
    }
}
/// PrivateKeyBundle encrypted with key material generated by
/// signing a randomly generated "pre-key" with the user's wallet,
/// i.e. EIP-191 signature of a "storage signature" message with
/// the pre-key embedded in it.
/// (see xmtp-js::PrivateKeyBundle.toEncryptedBytes for details)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EncryptedPrivateKeyBundleV1 {
    /// randomly generated pre-key 
    ///
    /// 32 bytes
    #[prost(bytes="vec", tag="1")]
    pub wallet_pre_key: ::prost::alloc::vec::Vec<u8>,
    /// MUST contain encrypted PrivateKeyBundle
    #[prost(message, optional, tag="2")]
    pub ciphertext: ::core::option::Option<Ciphertext>,
}
/// Versioned encrypted PrivateKeyBundle
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EncryptedPrivateKeyBundle {
    #[prost(oneof="encrypted_private_key_bundle::Version", tags="1")]
    pub version: ::core::option::Option<encrypted_private_key_bundle::Version>,
}
/// Nested message and enum types in `EncryptedPrivateKeyBundle`.
pub mod encrypted_private_key_bundle {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Version {
        #[prost(message, tag="1")]
        V1(super::EncryptedPrivateKeyBundleV1),
    }
}
include!("xmtp.message_contents.serde.rs");
// @@protoc_insertion_point(module)
//! A std-only reader for the legacy OpenMLS `GroupContext` record.
//!
//! Legacy SDKs stored the group context with bincode 1.3 (fixed-width
//! little-endian integers) and the serde tags of the xmtp OpenMLS fork at
//! `3fabbf23`. This reader keeps only what legacy metadata needs. It accepts
//! four extension types: RequiredCapabilities, ImmutableMetadata, Unknown, and
//! AppDataDictionary. XMTP groups never stored another type in a group context,
//! so another type, like malformed bytes, makes the record unreadable.
//!
//! ```text
//! GroupContext = ProtocolVersion, Ciphersuite, group_id: bytes, epoch: u64,
//!                tree_hash: bytes, confirmed_transcript_hash: bytes,
//!                extensions: Vec<Extension>
//! bytes        = u64 length, then the bytes
//! Vec<T>       = u64 count, then each T
//! enum         = u32 tag, then the variant's fields
//! Extension    = 2 RequiredCapabilities: Vec<ExtensionType>, Vec<ProposalType>,
//!                  Vec<CredentialType>
//!              | 6 ImmutableMetadata: bytes
//!              | 7 Unknown: u16 extension id, bytes
//!              | 8 AppDataDictionary: Vec<(u16 key, u16 component id, bytes)>
//! ```
//!
//! Trailing bytes are ignored, as the legacy decoder allowed them.

use std::collections::BTreeMap;

const EXTENSION_REQUIRED_CAPABILITIES: u32 = 2;
const EXTENSION_IMMUTABLE_METADATA: u32 = 6;
const EXTENSION_UNKNOWN: u32 = 7;
const EXTENSION_APP_DATA_DICTIONARY: u32 = 8;
/// The fork's `Ciphersuite` has this many variants with the
/// `draft-ietf-mls-pq-ciphersuites` feature. Serde stores the variant index.
const CIPHERSUITE_VARIANTS: u32 = 17;

/// The parts of a legacy group context that metadata reads.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LegacyContext<'a> {
    /// The first ImmutableMetadata extension.
    pub(crate) immutable_metadata: Option<&'a [u8]>,
    /// Every Unknown extension as (extension id, data), in stored order.
    pub(crate) unknown: Vec<(u16, &'a [u8])>,
    /// The entries of the first AppDataDictionary extension. A repeated key
    /// keeps its last value, as the legacy map decoder did.
    pub(crate) app_data: Option<BTreeMap<u16, &'a [u8]>>,
}

impl<'a> LegacyContext<'a> {
    /// Read a stored group context. `None` means the metadata is unavailable.
    pub(crate) fn read(bytes: &'a [u8]) -> Option<Self> {
        let mut reader = Reader(bytes);
        reader.protocol_version()?;
        if reader.u32()? >= CIPHERSUITE_VARIANTS {
            return None;
        }
        reader.bytes()?; // group_id
        reader.u64()?; // epoch
        reader.bytes()?; // tree_hash
        reader.bytes()?; // confirmed_transcript_hash
        let mut context = Self::default();
        for _ in 0..reader.u64()? {
            match reader.u32()? {
                EXTENSION_REQUIRED_CAPABILITIES => {
                    reader.each(Reader::extension_type)?;
                    reader.each(Reader::proposal_type)?;
                    reader.each(Reader::credential_type)?;
                }
                EXTENSION_IMMUTABLE_METADATA => {
                    let metadata = reader.bytes()?;
                    context.immutable_metadata.get_or_insert(metadata);
                }
                EXTENSION_UNKNOWN => {
                    let id = reader.u16()?;
                    context.unknown.push((id, reader.bytes()?));
                }
                EXTENSION_APP_DATA_DICTIONARY => {
                    let mut entries = BTreeMap::new();
                    for _ in 0..reader.u64()? {
                        let key = reader.u16()?;
                        reader.u16()?; // the component id stored again in the entry
                        entries.insert(key, reader.bytes()?);
                    }
                    context.app_data.get_or_insert(entries);
                }
                _ => return None,
            }
        }
        Some(context)
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(length)?;
        self.0 = tail;
        Some(head)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn u16(&mut self) -> Option<u16> {
        self.array().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Option<u64> {
        self.array().map(u64::from_le_bytes)
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.u64()?).ok()?;
        self.take(length)
    }

    /// Read a `Vec<T>` whose items `item` reads and discards.
    fn each(&mut self, item: fn(&mut Self) -> Option<()>) -> Option<()> {
        for _ in 0..self.u64()? {
            item(self)?;
        }
        Some(())
    }

    /// An enum tag below `variants`, followed by a u16 when the tag is in `with_u16`.
    fn tagged(&mut self, variants: u32, with_u16: [u32; 2]) -> Option<()> {
        let tag = self.u32()?;
        if tag >= variants {
            return None;
        }
        if with_u16.contains(&tag) {
            self.u16()?;
        }
        Some(())
    }

    /// 0 Mls10, 1 Other(u16).
    fn protocol_version(&mut self) -> Option<()> {
        let tag = self.u32()?;
        match tag {
            0 => Some(()),
            1 => self.u16().map(drop),
            _ => None,
        }
    }

    /// Tags 0-9: ApplicationId, RatchetTree, RequiredCapabilities, ExternalPub,
    /// ExternalSenders, LastResort, ImmutableMetadata, Unknown(u16),
    /// Grease(u16), AppDataDictionary.
    fn extension_type(&mut self) -> Option<()> {
        self.tagged(10, [7, 8])
    }

    /// Tags 0-6 Add to GroupContextExtensions, 8 SelfRemove, 9 Custom(u16),
    /// 10 Grease(u16), 11 AppEphemeral, 12 AppDataUpdate. Tag 7 is unused.
    fn proposal_type(&mut self) -> Option<()> {
        let tag = self.u32()?;
        match tag {
            9 | 10 => self.u16().map(drop),
            0..=6 | 8 | 11 | 12 => Some(()),
            _ => None,
        }
    }

    /// Tags 0 Basic, 1 X509, 2 Other(u16), 3 Grease(u16).
    fn credential_type(&mut self) -> Option<()> {
        self.tagged(4, [2, 3])
    }
}

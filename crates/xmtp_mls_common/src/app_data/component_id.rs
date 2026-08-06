use std::fmt;
use std::io::{Read, Write};

use tls_codec::{Deserialize, Serialize, Size};

// ============================================================================
// Component ID Ranges
// ============================================================================
//
// ComponentIds occupy the top half of the u16 space (0x8000-0xFFFF).
// The space is split between XMTP protocol and application use, with
// immutable ranges at the end of each block (counting down).

/// Start of the component ID space (top half of u16).
const COMPONENT_RANGE_START: u16 = 0x8000;

// --- XMTP Protocol Range: 0x8000-0xBFFF ---
const XMTP_RANGE_START: u16 = 0x8000;
const XMTP_RANGE_END: u16 = 0xBFFF;
/// Immutable XMTP components: 0xBE00-0xBFFF (512 IDs, counting down).
const XMTP_IMMUTABLE_START: u16 = 0xBE00;

// --- Application Range: 0xC000-0xFEFF ---
const APP_RANGE_START: u16 = 0xC000;
const APP_RANGE_END: u16 = 0xFEFF;
/// Immutable application components: 0xFD00-0xFEFF (512 IDs, counting down).
const APP_IMMUTABLE_START: u16 = 0xFD00;

// --- Reserved: 0xFF00-0xFFFF ---
const COMPONENT_RESERVED_START: u16 = 0xFF00;

// Compile-time invariant checks: if any of the range constants are ever
// changed in a way that breaks these assumptions, this will fail to build.
// The validation logic in `ComponentRegistry` relies on these — the ranges
// must be contiguous, non-overlapping, and immutable subranges must sit at
// the end of their parent block.
const _RANGE_INVARIANTS: () = {
    // The component space starts at exactly the XMTP range.
    assert!(XMTP_RANGE_START == COMPONENT_RANGE_START);
    assert!(XMTP_RANGE_START < XMTP_RANGE_END);
    // XMTP, App, and Reserved are contiguous and non-overlapping.
    assert!(APP_RANGE_START == XMTP_RANGE_END + 1);
    assert!(APP_RANGE_START < APP_RANGE_END);
    assert!(COMPONENT_RESERVED_START == APP_RANGE_END + 1);
    // Immutable subranges sit strictly inside their parent block, at the end.
    assert!(XMTP_IMMUTABLE_START > XMTP_RANGE_START);
    assert!(XMTP_IMMUTABLE_START <= XMTP_RANGE_END);
    assert!(APP_IMMUTABLE_START > APP_RANGE_START);
    assert!(APP_IMMUTABLE_START <= APP_RANGE_END);
};

/// A component identifier in the XMTP app data system.
///
/// ComponentIds occupy the top half of the u16 space (`0x8000-0xFFFF`), split
/// between XMTP protocol use (`0x8000-0xBFFF`) and application-defined
/// components (`0xC000-0xFEFF`), with `0xFF00-0xFFFF` reserved.
///
/// Immutable ranges sit at the end of each block (last 512 IDs, counting down).
/// Components in these ranges can be written once but never updated or deleted.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId(u16);

impl ComponentId {
    // === Hardcoded Component IDs ===
    // Permissions for these are enforced in code, not in the permissions map.

    /// The component registry. Super admin only.
    pub const COMPONENT_REGISTRY: Self = Self(0x8000);
    /// The super admin list. Super admin only.
    pub const SUPER_ADMIN_LIST: Self = Self(0x8001);
    /// The admin list. Configurable: super admin only or admin/super admin.
    pub const ADMIN_LIST: Self = Self(0x8002);

    // === Well-Known Mutable XMTP Component IDs (counting up from 0x8003) ===

    pub const GROUP_MEMBERSHIP: Self = Self(0x8003);
    pub const GROUP_NAME: Self = Self(0x8004);
    pub const GROUP_DESCRIPTION: Self = Self(0x8005);
    pub const GROUP_IMAGE_URL: Self = Self(0x8006);
    pub const MESSAGE_DISAPPEAR_FROM_NS: Self = Self(0x8007);
    pub const MESSAGE_DISAPPEAR_IN_NS: Self = Self(0x8008);
    pub const APP_DATA: Self = Self(0x8009);
    pub const MIN_SUPPORTED_PROTOCOL_VERSION: Self = Self(0x800A);
    pub const COMMIT_LOG_SIGNER: Self = Self(0x800B);

    // === Well-Known Immutable XMTP Component IDs (counting down from 0xBFFF) ===

    pub const CONVERSATION_TYPE: Self = Self(0xBFFF);
    pub const CREATOR_INBOX_ID: Self = Self(0xBFFE);
    pub const DM_MEMBERS: Self = Self(0xBFFD);
    pub const ONESHOT_MESSAGE: Self = Self(0xBFFC);

    // === Constructor and Accessors ===

    pub const fn new(id: u16) -> Self {
        Self(id)
    }

    pub const fn as_u16(self) -> u16 {
        self.0
    }

    // === Range Helpers ===

    /// Returns true if the ID is in the component ID space (top half of u16).
    /// Note: this includes the reserved range (`0xFF00-0xFFFF`); use
    /// [`is_reserved`](Self::is_reserved) to distinguish.
    pub const fn is_in_component_space(self) -> bool {
        self.0 >= COMPONENT_RANGE_START
    }

    /// Returns true if the ID is in the XMTP protocol range (`0x8000-0xBFFF`).
    pub const fn is_xmtp_range(self) -> bool {
        self.0 >= XMTP_RANGE_START && self.0 <= XMTP_RANGE_END
    }

    /// Returns true if the ID is in the application range (`0xC000-0xFEFF`).
    pub const fn is_app_range(self) -> bool {
        self.0 >= APP_RANGE_START && self.0 <= APP_RANGE_END
    }

    /// Returns true if the ID is in the reserved range (`0xFF00-0xFFFF`).
    pub const fn is_reserved(self) -> bool {
        self.0 >= COMPONENT_RESERVED_START
    }

    /// Returns true if the ID is in an immutable range.
    /// Immutable components can be inserted once but never updated or deleted.
    pub const fn is_immutable(self) -> bool {
        (self.0 >= XMTP_IMMUTABLE_START && self.0 <= XMTP_RANGE_END)
            || (self.0 >= APP_IMMUTABLE_START && self.0 <= APP_RANGE_END)
    }

    /// Returns true if this is one of the hardcoded components whose
    /// permissions are enforced in code rather than the component registry.
    pub const fn is_hardcoded(self) -> bool {
        self.0 == Self::COMPONENT_REGISTRY.0 || self.0 == Self::SUPER_ADMIN_LIST.0
    }

    /// Returns true if this component has constrained permission values.
    /// Constrained components can only have their permissions set to the
    /// proto base policies `AllowIfAdmin` (admin or super admin) or
    /// `AllowIfSuperAdmin` (super admin only).
    pub const fn is_constrained(self) -> bool {
        self.0 == Self::ADMIN_LIST.0
    }
}

impl fmt::Debug for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ComponentId(0x{:04X})", self.0)
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04X}", self.0)
    }
}

impl From<u16> for ComponentId {
    fn from(id: u16) -> Self {
        Self(id)
    }
}

impl From<ComponentId> for u16 {
    fn from(id: ComponentId) -> Self {
        id.0
    }
}

// === TLS Codec ===
//
// ComponentIds are encoded using QUIC variable-length integer encoding
// (RFC 9000 §16) rather than a fixed `u16`. This is forward-compatible:
// the underlying type can grow beyond `u16` in the future without breaking
// the wire format. The encoding uses 1, 2, 4, or 8 bytes depending on
// magnitude — current IDs (`0x8000-0xFFFF`) take 4 bytes.

/// Number of bytes a QUIC variable-length integer needs to encode `value`.
///
/// We delegate to `tls_codec::vlen::write_length` against a fixed-size stack
/// buffer rather than reimplementing the size table — that way our sizing
/// can never drift from what `tls_codec` actually emits, even if upstream
/// boundaries ever change. The 8-byte buffer is the maximum any QUIC vlen
/// encoding requires (RFC 9000 §16), so the write cannot fail with
/// `EndOfBuffer`.
fn vlen_encoding_bytes(value: usize) -> usize {
    let mut buf = [0u8; 8];
    let mut slice: &mut [u8] = &mut buf;
    tls_codec::vlen::write_length(&mut slice, value)
        .expect("8-byte buffer fits any QUIC vlen encoding")
}

impl Size for ComponentId {
    fn tls_serialized_len(&self) -> usize {
        vlen_encoding_bytes(self.0 as usize)
    }
}

impl Serialize for ComponentId {
    fn tls_serialize<W: Write>(&self, writer: &mut W) -> Result<usize, tls_codec::Error> {
        tls_codec::vlen::write_length(writer, self.0 as usize)
    }
}

impl Deserialize for ComponentId {
    fn tls_deserialize<R: Read>(bytes: &mut R) -> Result<Self, tls_codec::Error>
    where
        Self: Sized,
    {
        let (value, _len_len) = tls_codec::vlen::read_length(bytes)?;
        if value > u16::MAX as usize {
            return Err(tls_codec::Error::DecodingError(format!(
                "ComponentId value {value} exceeds u16::MAX; this version of \
                 the library cannot decode IDs larger than 0xFFFF"
            )));
        }
        Ok(Self(value as u16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tls_codec::{Deserialize, Serialize};

    #[xmtp_common::test]
    fn test_well_known_ids_are_in_expected_ranges() {
        // Hardcoded
        assert!(ComponentId::COMPONENT_REGISTRY.is_hardcoded());
        assert!(ComponentId::SUPER_ADMIN_LIST.is_hardcoded());
        assert!(!ComponentId::ADMIN_LIST.is_hardcoded());

        // Constrained
        assert!(ComponentId::ADMIN_LIST.is_constrained());
        assert!(!ComponentId::GROUP_NAME.is_constrained());

        // Mutable XMTP
        assert!(ComponentId::GROUP_MEMBERSHIP.is_xmtp_range());
        assert!(ComponentId::GROUP_NAME.is_xmtp_range());
        assert!(!ComponentId::GROUP_NAME.is_immutable());
        assert!(!ComponentId::GROUP_NAME.is_hardcoded());
        assert!(ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.is_xmtp_range());
        assert!(!ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION.is_immutable());
        assert!(ComponentId::COMMIT_LOG_SIGNER.is_xmtp_range());
        assert!(!ComponentId::COMMIT_LOG_SIGNER.is_immutable());

        // Immutable XMTP
        assert!(ComponentId::CONVERSATION_TYPE.is_immutable());
        assert!(ComponentId::CREATOR_INBOX_ID.is_immutable());
        assert!(ComponentId::CONVERSATION_TYPE.is_xmtp_range());
        assert!(ComponentId::DM_MEMBERS.is_immutable());
        assert!(ComponentId::DM_MEMBERS.is_xmtp_range());
        assert!(ComponentId::ONESHOT_MESSAGE.is_immutable());
        assert!(ComponentId::ONESHOT_MESSAGE.is_xmtp_range());
    }

    #[xmtp_common::test]
    fn test_range_boundaries() {
        // XMTP mutable (just after hardcoded)
        assert!(ComponentId::new(0x8003).is_xmtp_range());
        assert!(!ComponentId::new(0x8003).is_immutable());

        // The newest mutable XMTP IDs sit just past APP_DATA at 0x800A and 0x800B.
        assert!(ComponentId::new(0x800A).is_xmtp_range());
        assert!(!ComponentId::new(0x800A).is_immutable());
        assert!(ComponentId::new(0x800B).is_xmtp_range());
        assert!(!ComponentId::new(0x800B).is_immutable());

        // XMTP immutable boundary
        assert!(!ComponentId::new(0xBDFF).is_immutable());
        assert!(ComponentId::new(0xBE00).is_immutable());
        assert!(ComponentId::new(0xBFFF).is_immutable());

        // The newest immutable XMTP IDs (DM_MEMBERS, ONESHOT_MESSAGE) sit
        // counting down from 0xBFFF and must fall inside the immutable subrange.
        assert!(ComponentId::new(0xBFFD).is_immutable());
        assert!(ComponentId::new(0xBFFC).is_immutable());

        // App mutable
        assert!(ComponentId::new(0xC000).is_app_range());
        assert!(!ComponentId::new(0xC000).is_immutable());

        // App immutable boundary
        assert!(!ComponentId::new(0xFCFF).is_immutable());
        assert!(ComponentId::new(0xFD00).is_immutable());
        assert!(ComponentId::new(0xFEFF).is_immutable());

        // Reserved
        assert!(ComponentId::new(0xFF00).is_reserved());
        assert!(ComponentId::new(0xFFFF).is_reserved());
        assert!(!ComponentId::new(0xFEFF).is_reserved());
    }

    #[xmtp_common::test]
    fn test_is_in_component_space() {
        assert!(!ComponentId::new(0x0000).is_in_component_space());
        assert!(!ComponentId::new(0x7FFF).is_in_component_space());
        assert!(ComponentId::new(0x8000).is_in_component_space());
        assert!(ComponentId::new(0xFFFF).is_in_component_space());
    }

    #[xmtp_common::test]
    fn test_ranges_are_mutually_exclusive() {
        // XMTP and App ranges don't overlap
        for id in [0x8000u16, 0x8500, 0xBFFF] {
            let c = ComponentId::new(id);
            assert!(c.is_xmtp_range());
            assert!(!c.is_app_range());
            assert!(!c.is_reserved());
        }
        for id in [0xC000u16, 0xD000, 0xFEFF] {
            let c = ComponentId::new(id);
            assert!(!c.is_xmtp_range());
            assert!(c.is_app_range());
            assert!(!c.is_reserved());
        }
        for id in [0xFF00u16, 0xFFFF] {
            let c = ComponentId::new(id);
            assert!(!c.is_xmtp_range());
            assert!(!c.is_app_range());
            assert!(c.is_reserved());
        }
    }

    #[xmtp_common::test]
    fn test_tls_codec_round_trip() {
        // Round-trip every well-known id and a few representative values from
        // the various ranges. Component IDs are encoded with QUIC vlen so the
        // serialized length depends on magnitude.
        let ids = [
            ComponentId::new(0x0000),
            ComponentId::new(0x003F), // last 1-byte vlen value
            ComponentId::new(0x0040), // first 2-byte vlen value
            ComponentId::new(0x3FFF), // last 2-byte vlen value
            ComponentId::new(0x4000), // first 4-byte vlen value
            ComponentId::COMPONENT_REGISTRY,
            ComponentId::GROUP_NAME,
            ComponentId::MIN_SUPPORTED_PROTOCOL_VERSION,
            ComponentId::COMMIT_LOG_SIGNER,
            ComponentId::CONVERSATION_TYPE,
            ComponentId::DM_MEMBERS,
            ComponentId::ONESHOT_MESSAGE,
            ComponentId::new(0xFFFF),
        ];
        for id in ids {
            let bytes = id.tls_serialize_detached().unwrap();
            let deserialized = ComponentId::tls_deserialize_exact(&bytes).unwrap();
            assert_eq!(id, deserialized, "round trip failed for {id:?}");
            assert_eq!(
                bytes.len(),
                id.tls_serialized_len(),
                "tls_serialized_len mismatch for {id:?}"
            );
        }
    }

    #[xmtp_common::test]
    fn test_vlen_encoding_sizes() {
        // 1-byte vlen: 0..=0x3F
        assert_eq!(ComponentId::new(0).tls_serialized_len(), 1);
        assert_eq!(ComponentId::new(0x3F).tls_serialized_len(), 1);
        // 2-byte vlen: 0x40..=0x3FFF
        assert_eq!(ComponentId::new(0x40).tls_serialized_len(), 2);
        assert_eq!(ComponentId::new(0x3FFF).tls_serialized_len(), 2);
        // 4-byte vlen: 0x4000..
        assert_eq!(ComponentId::new(0x4000).tls_serialized_len(), 4);
        assert_eq!(ComponentId::new(0x8000).tls_serialized_len(), 4);
        assert_eq!(ComponentId::new(0xFFFF).tls_serialized_len(), 4);
    }

    #[xmtp_common::test]
    fn test_decode_rejects_value_above_u16_max() {
        // A QUIC vlen-encoded value of 0x10000 (one past u16::MAX) takes 4
        // bytes: prefix 0b10 (4-byte) || 0x00_01_00_00.
        let bytes = [0x80, 0x01, 0x00, 0x00];
        let result = ComponentId::tls_deserialize_exact(bytes);
        assert!(matches!(result, Err(tls_codec::Error::DecodingError(_))));
    }

    #[xmtp_common::test]
    fn test_ordering() {
        let a = ComponentId::new(0x8000);
        let b = ComponentId::new(0x8001);
        let c = ComponentId::new(0xBFFF);
        assert!(a < b);
        assert!(b < c);
    }

    #[xmtp_common::test]
    fn test_debug_display() {
        let id = ComponentId::new(0x8004);
        assert_eq!(format!("{id:?}"), "ComponentId(0x8004)");
        assert_eq!(format!("{id}"), "0x8004");
    }
}

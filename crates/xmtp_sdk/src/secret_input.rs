//! Secret arguments that UniFFI lifts straight into a wiping buffer.
//!
//! UniFFI lifts the arguments of an async export before it builds the
//! function's future, and the future owns them until its body runs. A host
//! that cancels the call before the first poll drops that future, so a plain
//! `Vec<u8>` or `String` key would be freed unwiped. These types wrap the
//! lifted value in [`Zeroizing`] inside the lift, before the future exists.
//!
//! Each type reports the type metadata of its builtin and reads the same wire
//! format, so the bindings see the same `bytes` or `string` parameter as
//! before: there is no custom type and no new foreign name.
//!
//! The lift also wipes the host's `RustBuffer`. [`SecretString`] takes the
//! buffer as wiping bytes, checks UTF-8 (UniFFI's builtin `String` lift does
//! not), and moves the checked bytes into the string. UniFFI lifts a `Vec<u8>`
//! by copying out of the buffer and then freeing it, so [`SecretBytes`] takes
//! the buffer itself, reads the copy, and wipes the buffer before it is freed. Both readers decode into wiping storage and
//! check the length (and UTF-8) before they build a value, so a malformed
//! buffer leaves no partial copy.
//!
//! [`SecretBytes`] is also an export return type: it lowers by writing into a
//! buffer sized up front and wiping its own storage, so the returned buffer is
//! the only copy. The builtin `Vec<u8>` lowering frees its source unwiped.

use std::{fmt, ops::Deref};

use uniffi::{
    Lift, Lower, LowerReturn, MetadataBuffer, RustBuffer, RustCallError, TypeId,
    deps::bytes::{Buf, BufMut},
};
use xmtp_cryptography::secret::Zeroizing;

macro_rules! secret_input {
    ($(#[$doc:meta])* $name:ident($builtin:ty) derefs to $target:ty) => {
        $(#[$doc])*
        pub struct $name(Zeroizing<$builtin>);

        impl From<$builtin> for $name {
            fn from(value: $builtin) -> Self {
                Self(Zeroizing::new(value))
            }
        }

        impl Deref for $name {
            type Target = $target;

            fn deref(&self) -> &$target {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }

        impl<UT> TypeId<UT> for $name {
            const TYPE_ID_META: MetadataBuffer = <$builtin as TypeId<UT>>::TYPE_ID_META;
        }
    };
}

secret_input!(
    /// Secret bytes passed in from a host, as a `bytes` parameter. Wiped on drop.
    SecretBytes(Vec<u8>) derefs to [u8]
);

secret_input!(
    /// Secret text passed in from a host, as a `string` parameter. Wiped on drop.
    SecretString(String) derefs to str
);

/// Read one length-prefixed byte string, the wire format of both `Vec<u8>`
/// and `String`, straight into wiping storage. The length is checked before
/// anything is copied, so a truncated buffer fails without a partial copy.
fn read_secret(buf: &mut &[u8]) -> uniffi::Result<Zeroizing<Vec<u8>>> {
    uniffi::check_remaining(buf, 4)?;
    let len = usize::try_from(buf.get_i32())?;
    uniffi::check_remaining(buf, len)?;
    let mut out = Zeroizing::new(Vec::with_capacity(len));
    out.extend_from_slice(&buf[..len]);
    buf.advance(len);
    Ok(out)
}

/// Turn the host's `RustBuffer` into `T`, wiping the buffer afterwards. Junk
/// after the value is an error, as in the builtin lift.
fn lift_from_buffer<T>(
    value: RustBuffer,
    read: impl FnOnce(&mut &[u8]) -> uniffi::Result<T>,
) -> uniffi::Result<T> {
    let source = Zeroizing::new(value.destroy_into_vec());
    let mut buf = source.as_slice();
    let lifted = read(&mut buf)?;
    if !buf.is_empty() {
        uniffi::deps::anyhow::bail!(
            "junk data left in buffer after lifting (count: {})",
            buf.len()
        );
    }
    Ok(lifted)
}

// SAFETY: same FFI type and wire format as `Vec<u8>`, and `TypeId` reports the
// builtin's metadata, so the bindings pass the argument exactly as `Vec<u8>`.
unsafe impl<UT> Lift<UT> for SecretBytes {
    type FfiType = RustBuffer;

    /// Owns the host's buffer while it reads the bytes, then wipes it. The
    /// builtin lift would free that buffer, which holds the secret, unwiped.
    fn try_lift(value: RustBuffer) -> uniffi::Result<Self> {
        lift_from_buffer(value, read_secret).map(Self)
    }

    fn try_read(buf: &mut &[u8]) -> uniffi::Result<Self> {
        read_secret(buf).map(Self)
    }
}

// SAFETY: same FFI type and wire format as `Vec<u8>`, and `TypeId` reports the
// builtin's metadata, so the bindings receive the value exactly as `Vec<u8>`.
unsafe impl<UT> Lower<UT> for SecretBytes {
    type FfiType = RustBuffer;

    /// Writes into a buffer sized up front, so it never reallocates; the
    /// secret's own storage is wiped when `obj` drops.
    fn write(obj: Self, buf: &mut Vec<u8>) {
        // A secret never approaches `i32::MAX` bytes.
        let len = i32::try_from(obj.len()).expect("secret length fits in i32");
        buf.put_i32(len);
        buf.put_slice(&obj);
    }

    /// The returned buffer is the export, owned by the host from here on.
    fn lower(obj: Self) -> RustBuffer {
        let mut buf = Vec::with_capacity(4 + obj.len());
        <Self as Lower<UT>>::write(obj, &mut buf);
        RustBuffer::from_vec(buf)
    }
}

// SAFETY: returns the `Lower` value unchanged, as the derived impl does.
unsafe impl<UT> LowerReturn<UT> for SecretBytes {
    type ReturnType = RustBuffer;

    fn lower_return(value: Self) -> Result<RustBuffer, RustCallError> {
        Ok(<Self as Lower<UT>>::lower(value))
    }
}

// SAFETY: same FFI type and wire format as `String`, and `TypeId` reports the
// builtin's metadata, so the bindings pass the argument exactly as `String`.
unsafe impl<UT> Lift<UT> for SecretString {
    type FfiType = RustBuffer;

    /// Owns the host's buffer as wiping bytes and checks UTF-8 before it
    /// becomes the `String`. UniFFI's builtin `String` lift skips that check.
    /// The checked bytes move into the `String` without a copy, and invalid
    /// bytes are wiped with the buffer.
    fn try_lift(value: RustBuffer) -> uniffi::Result<Self> {
        utf8_secret(Zeroizing::new(value.destroy_into_vec()))
    }

    fn try_read(buf: &mut &[u8]) -> uniffi::Result<Self> {
        utf8_secret(read_secret(buf)?)
    }
}

/// Check `bytes` as UTF-8, then move them into a `SecretString`.
fn utf8_secret(mut bytes: Zeroizing<Vec<u8>>) -> uniffi::Result<SecretString> {
    std::str::from_utf8(&bytes)?;
    let text = String::from_utf8(std::mem::take(&mut *bytes)).expect("checked as UTF-8 above");
    Ok(SecretString::from(text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UniFfiTag;

    /// The bindings read a parameter's type from this metadata. Equal bytes
    /// mean the foreign signature of every function that takes these types is
    /// unchanged.
    #[xmtp_common::test(unwrap_try = true)]
    fn metadata_matches_the_builtin() {
        assert_eq!(
            <SecretBytes as TypeId<UniFfiTag>>::TYPE_ID_META.as_ref(),
            <Vec<u8> as TypeId<UniFfiTag>>::TYPE_ID_META.as_ref()
        );
        assert_eq!(
            <SecretString as TypeId<UniFfiTag>>::TYPE_ID_META.as_ref(),
            <String as TypeId<UniFfiTag>>::TYPE_ID_META.as_ref()
        );
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn lifts_like_the_builtin() {
        let bytes = <Vec<u8> as uniffi::Lower<UniFfiTag>>::lower(vec![1, 2, 3]);
        let lifted = <SecretBytes as Lift<UniFfiTag>>::try_lift(bytes)?;
        assert_eq!(&*lifted, [1, 2, 3]);

        let text = <String as uniffi::Lower<UniFfiTag>>::lower("0xab".to_owned());
        let lifted = <SecretString as Lift<UniFfiTag>>::try_lift(text)?;
        assert_eq!(&*lifted, "0xab");
    }

    /// UniFFI's builtin `String` lift does not check UTF-8; this one does, so
    /// a malformed host buffer is an error, not an invalid `String`.
    #[xmtp_common::test(unwrap_try = true)]
    fn rejects_invalid_utf8() {
        let buffer = RustBuffer::from_vec(vec![b'0', b'x', 0xff]);
        assert!(<SecretString as Lift<UniFfiTag>>::try_lift(buffer).is_err());
    }

    #[xmtp_common::test(unwrap_try = true)]
    fn debug_is_redacted() {
        assert_eq!(
            format!("{:?}", SecretBytes::from(vec![7; 32])),
            "SecretBytes([REDACTED])"
        );
        assert_eq!(
            format!("{:?}", SecretString::from("0x07".to_owned())),
            "SecretString([REDACTED])"
        );
    }
}

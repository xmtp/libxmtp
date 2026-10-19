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
//! The lift also wipes the host's `RustBuffer`. UniFFI lifts a `String` by
//! reusing that buffer, so the `Zeroizing<String>` wipes it. It lifts a
//! `Vec<u8>` by copying out of the buffer and then freeing it, so
//! [`SecretBytes`] takes the buffer itself, reads the copy, and wipes the
//! buffer before it is freed.

use std::{fmt, ops::Deref};

use uniffi::{Lift, MetadataBuffer, RustBuffer, TypeId};
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

// SAFETY: same FFI type and wire format as `Vec<u8>`, and `TypeId` reports the
// builtin's metadata, so the bindings pass the argument exactly as `Vec<u8>`.
unsafe impl<UT> Lift<UT> for SecretBytes {
    type FfiType = RustBuffer;

    /// Owns the host's buffer while it reads the bytes, then wipes it. The
    /// builtin lift would free that buffer, which holds the secret, unwiped.
    fn try_lift(value: RustBuffer) -> uniffi::Result<Self> {
        let source = Zeroizing::new(value.destroy_into_vec());
        let mut buf = source.as_slice();
        let lifted = <Self as Lift<UT>>::try_read(&mut buf)?;
        if !buf.is_empty() {
            uniffi::deps::anyhow::bail!(
                "junk data left in buffer after lifting (count: {})",
                buf.len()
            );
        }
        Ok(lifted)
    }

    /// The builtin reads into a `Vec` sized to the length prefix, so it never
    /// reallocates; the result is wrapped at once.
    fn try_read(buf: &mut &[u8]) -> uniffi::Result<Self> {
        <Vec<u8> as Lift<UT>>::try_read(buf).map(Self::from)
    }
}

// SAFETY: forwards to `String`'s `Lift`, with the same FFI type. That lift
// reuses the host's buffer as the `String`, which `Zeroizing` then wipes.
unsafe impl<UT> Lift<UT> for SecretString {
    type FfiType = <String as Lift<UT>>::FfiType;

    fn try_lift(value: Self::FfiType) -> uniffi::Result<Self> {
        <String as Lift<UT>>::try_lift(value).map(Self::from)
    }

    fn try_read(buf: &mut &[u8]) -> uniffi::Result<Self> {
        <String as Lift<UT>>::try_read(buf).map(Self::from)
    }
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

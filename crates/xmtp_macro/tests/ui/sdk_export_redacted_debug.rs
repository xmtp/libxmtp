// sdk_export implements Debug for a type with a redacted field, so a derived
// Debug conflicts with it however it is written.
use std::fmt::Debug as Printable;

uniffi::setup_scaffolding!();

// A derive above sdk_export expands before it, out of the macro's sight.
#[derive(Debug)]
#[xmtp_macro::sdk_export]
#[derive(Clone, uniffi::Record)]
pub struct Above {
    #[sdk(redact)]
    pub key: Vec<u8>,
}

#[xmtp_macro::sdk_export]
#[derive(Clone, uniffi::Record)]
#[cfg_attr(all(), derive(Debug))]
pub struct Conditional {
    #[sdk(redact)]
    pub key: Vec<u8>,
}

#[xmtp_macro::sdk_export]
#[derive(Clone, Printable, uniffi::Record)]
pub struct Renamed {
    #[sdk(redact)]
    pub key: Vec<u8>,
}

#[derive(Debug)]
#[xmtp_macro::sdk_export]
#[derive(Clone, uniffi::Enum)]
pub enum Channel {
    Apns {
        #[sdk(redact)]
        token: String,
    },
}

macro_rules! redacted_debug {
    ($($name:ident),*) => {$(
        impl $name {
            fn redacted_debug(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(stringify!($name))
            }
        }
    )*};
}

redacted_debug!(Above, Conditional, Renamed, Channel);

fn main() {}

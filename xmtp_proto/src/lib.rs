#[allow(clippy::all)]
#[allow(warnings)]
mod generated {
    include!("gen/mod.rs");
}
pub use generated::*;

pub mod mls {
    pub mod api {
        pub mod v1 {
            pub mod prelude {
                pub use crate::xmtp::mls::api::v1::*;
            }
        }
    }
}

pub mod identity {
    pub mod api {
        pub mod v1 {
            pub mod prelude {
                pub use crate::xmtp::identity::api::v1::*;
            }
        }
    }
}

mod error;
pub use error::*;

pub mod api_client;
pub mod traits;

#[cfg(feature = "convert")]
pub mod convert;
#[cfg(feature = "convert")]
pub mod types;
#[cfg(feature = "convert")]
pub mod v4_utils;

#[cfg(test)]
pub mod test {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
}

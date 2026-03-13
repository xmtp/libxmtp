#![recursion_limit = "256"]
xmtp_common::if_native! {
    pub mod crypto;
    pub mod fork_recovery;
    pub mod identity;
    pub mod inbox_owner;
    pub mod logger;
    pub mod message;
    pub mod mls;
    pub mod worker;
    pub mod native;

    #[cfg(test)]
    mod builder_test;

    pub use native::*;
    pub use message::*;
    pub use mls::*;
    pub use logger::{enter_debug_writer, exit_debug_writer};


    extern crate tracing as log;
}

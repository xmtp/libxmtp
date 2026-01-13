//! xnet - XMTP Network Testing Framework
//!
//! Shared library for managing Docker containers for XMTP testing.

// Allow unused code during development - this crate is a work in progress
#![allow(dead_code, unused_imports, unused_variables)]

pub mod app;
pub mod config;
pub mod constants;
pub mod dns_setup;
pub mod network;
pub mod services;
pub mod types;
pub mod xmtpd_cli;

pub use config::Config;

#[macro_use]
extern crate tracing;

pub fn get_version() -> String {
    format!("{}-{}", env!("CARGO_PKG_VERSION"), env!("VERGEN_GIT_SHA"))
}

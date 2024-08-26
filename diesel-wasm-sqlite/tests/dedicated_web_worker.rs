//! Integration Test Organization in rust is little bit non-standard comapred to normal
//! organization in a library/binary.
//! In order to avoid being compiled as a test, common functions must be defined in
//! `common/mod.rs`
//!
//! Tests are separated into separate files in the module `test`.
#![recursion_limit = "256"]
#![cfg(target_arch = "wasm32")]

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

mod common;

mod test {
    mod row;
    mod web;
}

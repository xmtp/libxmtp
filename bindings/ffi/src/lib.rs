// Copyright 2022 The Matrix.org Foundation C.I.C.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

uniffi_macros::include_scaffolding!("my_rust_code");

pub use corecrypto::encryption;

pub fn decrypt(
    ciphertext_bytes: Vec<u8>,
    salt_bytes: Vec<u8>,
    nonce_bytes: Vec<u8>,
    secret_bytes: Vec<u8>,
    additional_data: Vec<u8>,
) -> Vec<u8> {
    // If additional_data is empty,then pass None, by shadowing variable
    let additional_data = if additional_data.is_empty() {
        None
    } else {
        Some(additional_data.as_slice())
    };
    encryption::decrypt(
        &ciphertext_bytes,
        &salt_bytes,
        &nonce_bytes,
        &secret_bytes,
        additional_data,
    ).unwrap()
}

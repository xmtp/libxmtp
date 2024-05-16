// use serde::{Deserialize, Serialize};
// use xmtp_cryptography::signature::{RecoverableSignature, SignatureError};
//
// #[derive(Debug, Serialize, Deserialize, thiserror::Error)]
// pub enum SigningError {
//     #[error("This is a generic error")]
//     Generic,
// }
//
// // A simplified InboxOwner passed to Rust across the WASM boundary
// pub trait WasmInboxOwner: Send + Sync {
//     fn get_address(&self) -> String;
//     fn sign(&self, text: String) -> Result<Vec<u8>, SigningError>;
// }
//
// pub struct RustInboxOwner {
//     wasm_inbox_owner: Box<dyn WasmInboxOwner>,
// }
//
// impl RustInboxOwner {
//     pub fn new(wasm_inbox_owner: Box<dyn WasmInboxOwner>) -> Self {
//         Self { wasm_inbox_owner }
//     }
// }
//
// impl xmtp_mls::InboxOwner for RustInboxOwner {
//     fn get_address(&self) -> String {
//         self.wasm_inbox_owner.get_address().to_lowercase()
//     }
//
//     fn sign(&self, text: &str) -> Result<RecoverableSignature, SignatureError> {
//         let bytes = self
//             .wasm_inbox_owner
//             .sign(text.to_string())
//             .map_err(|_flat_err| SignatureError::Unknown)?;
//         Ok(RecoverableSignature::Eip191Signature(bytes))
//     }
// }

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Diagnostic {
    // decrypted message strings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<String>>,
    // general count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

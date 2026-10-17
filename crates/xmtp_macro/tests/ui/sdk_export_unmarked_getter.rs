uniffi::setup_scaffolding!();

#[derive(uniffi::Object)]
struct Api;

#[xmtp_macro::sdk_export]
impl Api {
    pub fn inbox_id(&self) -> String {
        String::new()
    }
}

fn main() {}

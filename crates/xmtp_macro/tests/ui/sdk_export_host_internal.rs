use xmtp_macro::sdk_export;

#[sdk_export]
#[derive(uniffi::Record)]
struct Record {
    #[sdk(host_internal)]
    value: u64,
}

#[sdk_export]
impl Object {
    #[sdk(host_internal, host_internal)]
    pub async fn repeated(&self) {}
}

#[sdk_export]
impl Object {
    #[doc = "@xmtp-host-internal"]
    pub async fn written(&self) {}
}

fn main() {}

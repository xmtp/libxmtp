use xmtp_macro::sdk_export;

#[sdk_export]
impl Object {
    #[sdk(stream(
        name = "consume",
        options = "MessageStreamOptions",
        owner = "key",
        extra = "value"
    ))]
    pub async fn unknown(&self) {}
}

#[sdk_export]
impl Object {
    #[sdk(stream(name = "consume", options = "MessageStreamOptions", owner = "key"))]
    pub fn sync_reader(&self) {}
}

#[sdk_export]
impl Object {
    #[doc = "@xmtp-stream=consume:MessageStreamOptions:key"]
    pub async fn written(&self) {}
}

fn main() {}

pub struct ReplyCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-reply/src/Reply.ts
impl ReplyCodec {
    pub const TYPE_ID: &'static str = "reply";
}

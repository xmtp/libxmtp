pub struct RemoteAttachmentCodec {}

//. Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-remote-attachment/src/RemoteAttachment.ts
impl RemoteAttachmentCodec {
    pub const TYPE_ID: &'static str = "remoteStaticAttachment";
}

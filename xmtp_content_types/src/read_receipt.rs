pub struct ReadReceiptCodec {}

impl ReadReceiptCodec {
    const AUTHORITY_ID: &'static str = "xmtp.org";
    pub const TYPE_ID: &'static str = "read_receipt";
}

pub struct ReadReceiptCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-read-receipt/src/ReadReceipt.ts
impl ReadReceiptCodec {
    pub const TYPE_ID: &'static str = "readReceipt";
}

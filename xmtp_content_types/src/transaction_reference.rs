pub struct TransactionReferenceCodec {}

/// Legacy content type id at https://github.com/xmtp/xmtp-js/blob/main/content-types/content-type-transaction-reference/src/TransactionReference.ts
impl TransactionReferenceCodec {
    pub const TYPE_ID: &'static str = "transactionReference";
}

package org.xmtp.android.library.codecs

val ContentTypeTransactionReference =
    ContentTypeIdBuilder.builderFromAuthorityId(
        "xmtp.org",
        "transactionReference",
        versionMajor = 1,
        versionMinor = 0,
    )

data class TransactionReference(
    val namespace: String? = null,
    val networkId: String,
    val reference: String,
    val metadata: Metadata? = null,
) {
    data class Metadata(
        val transactionType: String,
        val currency: String,
        val amount: Double,
        val decimals: UInt,
        val fromAddress: String,
        val toAddress: String,
    )
}

data class TransactionReferenceCodec(
    override var contentType: ContentTypeId = ContentTypeTransactionReference,
) : ContentCodec<TransactionReference> {
    override fun encode(content: TransactionReference): EncodedContent {
        val ffi =
            uniffi.xmtpv3.FfiTransactionReference(
                namespace = content.namespace,
                networkId = content.networkId,
                reference = content.reference,
                metadata =
                    content.metadata?.let {
                        uniffi.xmtpv3.FfiTransactionMetadata(
                            transactionType = it.transactionType,
                            currency = it.currency,
                            amount = it.amount,
                            decimals = it.decimals,
                            fromAddress = it.fromAddress,
                            toAddress = it.toAddress,
                        )
                    },
            )

        return EncodedContent.parseFrom(
            uniffi.xmtpv3.encodeTransactionReference(ffi),
        )
    }

    override fun decode(content: EncodedContent): TransactionReference {
        val decoded = uniffi.xmtpv3.decodeTransactionReference(content.toByteArray())

        return TransactionReference(
            namespace = decoded.namespace,
            networkId = decoded.networkId,
            reference = decoded.reference,
            metadata =
                decoded.metadata?.let {
                    TransactionReference.Metadata(
                        transactionType = it.transactionType,
                        currency = it.currency,
                        amount = it.amount,
                        decimals = it.decimals,
                        fromAddress = it.fromAddress,
                        toAddress = it.toAddress,
                    )
                },
        )
    }

    override fun fallback(content: TransactionReference): String =
        "[Crypto transaction] Use a blockchain explorer to learn more using the transaction hash: ${content.reference}"

    override fun shouldPush(content: TransactionReference): Boolean = true
}

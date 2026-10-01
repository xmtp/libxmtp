import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.withTimeout
import uniffi.xmtp_sdk.*

// verifies: CTYPE-008, CTYPE-009, CTYPE-027, CTYPE-029
internal fun checkRetainedContent(
    failed: Message,
    nestedFailure: Message,
) {
    val custom = failed.content as? SDKMessageContent.Custom ?: error("custom failure missing")
    check(custom.rawBytes.contentEquals(failed.rawBytes) && custom.rawBytes.isNotEmpty())
    check(custom.value == null)
    check(
        custom.error?.let {
            it.code == "CodecDecodeFailed" && it.category == ErrorCategory.CALLBACK && !it.retryable &&
                it.message.contains("codec decode failed")
        } ==
            true,
    )
    val nested = nestedFailure.content as? SDKMessageContent.Unknown ?: error("outer Unknown missing")
    check(nested.rawBytes.contentEquals(nestedFailure.rawBytes) && nested.rawBytes.isNotEmpty())
    check(nested.encoded?.fallback == nestedFailure.fallback)
    check(
        nested.error.code == "CodecDecodeFailed" && nested.error.category == ErrorCategory.CALLBACK &&
            !nested.error.retryable,
    )

    val raw = byteArrayOf(-1, -128)
    val details = ErrorDetails("MalformedEnvelope", ErrorCategory.INPUT, false, "invalid protobuf")
    val data =
        failed.data.copy(
            rawBytes = raw,
            contentType = null,
            encoded = null,
            fallback = null,
            content = MessageContent.Unknown(null, raw, details),
        )
    val malformed = Message(data)
    check(malformed.contentType == null && malformed.encoded == null && malformed.rawBytes.contentEquals(raw))
    val unknown = malformed.content as? SDKMessageContent.Unknown ?: error("malformed Unknown missing")
    check(unknown.encoded == null && unknown.rawBytes.contentEquals(raw))
    check(
        unknown.error.code == "MalformedEnvelope" && unknown.error.category == ErrorCategory.INPUT &&
            !unknown.error.retryable,
    )
    val copy =
        Message(data.copy(rawBytes = raw.copyOf(), content = MessageContent.Unknown(null, raw.copyOf(), details)))
    check(
        malformed == copy && malformed.hashCode() == copy.hashCode(),
    ) { "retained content equality uses array identity" }
    check(malformed != Message(data.copy(rawBytes = byteArrayOf(1)))) { "message raw bytes do not affect equality" }
    check(
        malformed !=
            Message(data.copy(content = MessageContent.Unknown(null, raw, details.copy(message = "other failure")))),
    ) { "failure details do not affect equality" }

    val encoded = checkNotNull(failed.encoded)
    val parent =
        ReplyParent(
            failed.id,
            failed.senderInboxId,
            failed.sentAt,
            failed.kind,
            failed.deliveryStatus,
            failed.rawBytes,
            failed.contentType,
            failed.fallback,
            encoded,
            MessageBody.Custom(encoded, failed.rawBytes),
        )
    val withFailedParent = Message(data.copy(content = MessageContent.Text("valid reply"), inReplyTo = parent))
    check((withFailedParent.content as? SDKMessageContent.Standard)?.value == MessageContent.Text("valid reply"))
    val parentContent = withFailedParent.inReplyToContent as? SDKReplyContent.Custom ?: error("parent custom missing")
    check(parentContent.rawBytes.contentEquals(failed.rawBytes) && parentContent.error?.code == "CodecDecodeFailed")
}

private class HostileCodecException : RuntimeException("hostile codec failure") {
    override fun toString(): String = throw IllegalStateException("hostile diagnostic conversion")
}

private class HostileDiagnosticCodec : ContentCodec<String> {
    override val type = SampleCodec().type

    override fun encode(value: String) = SampleCodec().encode(value)

    override fun decode(encoded: EncodedContent): String {
        val value = encoded.content.decodeToString()
        if (value == "hostile codec failure") throw HostileCodecException()
        return value
    }
}

// verifies: CTYPE-008, CTYPE-029, PROC-045
internal suspend fun checkHostileCodecStream(
    identity: PublicIdentity,
    options: ClientOptions,
    inboxId: InboxId,
) {
    val codec = HostileDiagnosticCodec()
    val host = SDKClient.build(identity, options, inboxId, codecs = listOf(codec))
    try {
        val group = host.conversations().createGroup(emptyList(), null)
        val failedId = group.send(codec.encode("hostile codec failure"), null)
        val validId = group.send(codec.encode("after hostile codec failure"), null)
        val items =
            try {
                withTimeout(10_000) { host.messages(group).take(2).toList() }
            } catch (error: Throwable) {
                throw AssertionError("hostile codec diagnostic escaped the stream: ${error.javaClass.name}")
            }
        check(items.map { it.id } == listOf(failedId, validId))
        val failed = items[0].content as? SDKMessageContent.Custom ?: error("hostile custom failure missing")
        check(failed.rawBytes.contentEquals(items[0].rawBytes) && failed.rawBytes.isNotEmpty())
        check(failed.encoded.content.contentEquals(codec.encode("hostile codec failure").content))
        check(failed.value == null)
        check(
            failed.error?.let {
                it.code == "CodecDecodeFailed" && it.category == ErrorCategory.CALLBACK && !it.retryable &&
                    it.message == "custom content codec failed"
            } == true,
        )
        val valid = items[1].content as? SDKMessageContent.Custom ?: error("valid custom item missing")
        check(valid.value == "after hostile codec failure" && valid.error == null)
        println("Kotlin hostile_codec_diagnostic_keeps_stream_open passed")
    } finally {
        host.end()
    }
}

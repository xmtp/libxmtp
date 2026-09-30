package uniffi.xmtp_sdk

import kotlin.coroutines.cancellation.CancellationException

// The host send policy for typed codecs (Ref Public surface, Host codecs;
// Decisions 23 and 24). Every codec step runs before the send starts, so a
// failed step makes no publish attempt.

private fun codecEncodeFailed(
    step: String,
    reason: String,
) = XmtpException.CodecEncodeFailed(
    ErrorDetails("CodecEncodeFailed", ErrorCategory.CALLBACK, false, "content codec $step failed: $reason"),
)

private inline fun <R> codecStep(
    name: String,
    run: () -> R,
): R =
    try {
        run()
    } catch (error: CancellationException) {
        throw error
    } catch (error: Throwable) {
        throw codecEncodeFailed(name, error.message ?: error.toString())
    }

/**
 * The envelope of [value] for a send. An envelope that already has a fallback
 * keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
 * it. A failed step, or an envelope of another type than the codec's, is
 * `CodecEncodeFailed`.
 */
internal fun <T : Any> encodeForSend(
    codec: ContentCodec<T>,
    value: T,
): EncodedContent {
    val encoded = codecStep("encode") { codec.encode(value) }
    // implements: CTYPE-007
    // The envelope type is the codec's type, so a codec's push hook cannot
    // steer catalogue dispatch.
    if (encoded.type != codec.type) {
        throw codecEncodeFailed("encode", "the envelope type differs from the codec type")
    }
    if (encoded.fallback != null) return encoded
    val fallback = codecStep("fallback") { codec.fallback(value) } ?: return encoded
    return encoded.copy(fallback = fallback)
}

/**
 * The send options for [value]. An explicit `shouldPush`, including `false`,
 * wins. A catalogue type keeps its catalogue default. Otherwise the codec's
 * `shouldPush` decides. A failed hook is `CodecEncodeFailed`.
 */
internal fun <T : Any> optionsForSend(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions?,
): SendOptions? {
    if (options?.shouldPush != null || isCatalogueContentType(codec.type)) return options
    val push = codecStep("shouldPush") { codec.shouldPush(value) }
    return (options ?: SendOptions()).copy(shouldPush = push)
}

// Decision 23: a typed codec form of send and prepareMessage next to the
// envelope form.

suspend fun <T : Any> Group.send(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val encoded = encodeForSend(codec, value)
    return send(encoded, optionsForSend(codec, value, options))
}

suspend fun <T : Any> Group.prepareMessage(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val encoded = encodeForSend(codec, value)
    return prepareMessage(encoded, optionsForSend(codec, value, options))
}

suspend fun <T : Any> Dm.send(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val encoded = encodeForSend(codec, value)
    return send(encoded, optionsForSend(codec, value, options))
}

suspend fun <T : Any> Dm.prepareMessage(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val encoded = encodeForSend(codec, value)
    return prepareMessage(encoded, optionsForSend(codec, value, options))
}

suspend fun <T : Any> Conversation.send(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val encoded = encodeForSend(codec, value)
    return send(encoded, optionsForSend(codec, value, options))
}

suspend fun <T : Any> Conversation.prepareMessage(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val encoded = encodeForSend(codec, value)
    return prepareMessage(encoded, optionsForSend(codec, value, options))
}

@file:JvmName("ContentCodecSends")

package uniffi.xmtp_sdk

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

// The host send policy for typed codecs (Ref Public surface, Host codecs;
// Decisions 23 and 24). Every codec step runs before the send starts, so a
// failed step makes no publish attempt.

private fun codecEncodeFailed(
    step: String,
    reason: String,
) = XmtpException.CodecEncodeFailed(
    ErrorDetails("CodecEncodeFailed", ErrorCategory.CALLBACK, false, "content codec $step failed: $reason"),
)

// A codec step is not a suspend function, so anything it throws comes from the
// codec, including a CancellationException, a NotImplementedError from TODO(),
// or an AssertionError. Each one is CodecEncodeFailed. Only a
// VirtualMachineError, such as OutOfMemoryError, passes through.
private inline fun <R> codecStep(
    name: String,
    run: () -> R,
): R =
    try {
        run()
    } catch (error: VirtualMachineError) {
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

/**
 * The envelope of [value] for a reply. The caller's cancellation is checked
 * before the codec steps run.
 */
internal suspend fun <T : Any> replyEnvelope(
    codec: ContentCodec<T>,
    value: T,
): EncodedContent {
    currentCoroutineContext().ensureActive()
    return encodeForSend(codec, value)
}

/**
 * The envelope and options of [value] for a send or prepare. The caller's
 * cancellation is checked before the codec steps run.
 */
private suspend fun <T : Any> sendParts(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions?,
): Pair<EncodedContent, SendOptions?> {
    currentCoroutineContext().ensureActive()
    return encodeForSend(codec, value) to optionsForSend(codec, value, options)
}

// Decision 23: a typed codec form of send and prepareMessage next to the
// envelope form.

suspend fun <T : Any> Group.send(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val (encoded, sendOptions) = sendParts(codec, value, options)
    return send(encoded, sendOptions)
}

suspend fun <T : Any> Group.prepareMessage(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val (encoded, sendOptions) = sendParts(codec, value, options)
    return prepareMessage(encoded, sendOptions)
}

suspend fun <T : Any> Dm.send(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val (encoded, sendOptions) = sendParts(codec, value, options)
    return send(encoded, sendOptions)
}

suspend fun <T : Any> Dm.prepareMessage(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val (encoded, sendOptions) = sendParts(codec, value, options)
    return prepareMessage(encoded, sendOptions)
}

suspend fun <T : Any> Conversation.send(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val (encoded, sendOptions) = sendParts(codec, value, options)
    return send(encoded, sendOptions)
}

suspend fun <T : Any> Conversation.prepareMessage(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions? = null,
): MessageId {
    val (encoded, sendOptions) = sendParts(codec, value, options)
    return prepareMessage(encoded, sendOptions)
}

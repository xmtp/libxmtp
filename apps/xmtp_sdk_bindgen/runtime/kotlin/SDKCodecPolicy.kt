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
// VirtualMachineError, such as OutOfMemoryError, passes through. Swift differs
// on purpose: a Swift step runs inside the caller's task, so there a
// CancellationError reports the caller's cancellation and passes through. Here
// the caller's cancellation is checked with ensureActive() around the steps.
private inline fun <R> codecStep(
    name: String,
    run: () -> R,
): R =
    try {
        run()
    } catch (error: VirtualMachineError) {
        throw error
    } catch (error: Throwable) {
        throw codecEncodeFailed(name, describe(error))
    }

// A description of a codec failure. Reading `message` or `toString()` can
// itself throw, so it has a fixed fallback.
private fun describe(error: Throwable): String =
    try {
        error.message ?: error.toString()
    } catch (fatal: VirtualMachineError) {
        throw fatal
    } catch (_: Throwable) {
        "the failure has no readable description"
    }

/** The codec's content type, read once under its own step. */
internal fun <T : Any> codecType(codec: ContentCodec<T>): ContentTypeId = codecStep("type") { codec.type }

// A copy of the codec's envelope with its own type and parameters. The
// generated record has `var` fields, so a hook that keeps and changes the
// codec's envelope object cannot change the checked copy. The content bytes
// are not copied: they do not decide the type or the push policy.
private fun EncodedContent.snapshot(): EncodedContent = copy(type = type.copy(), parameters = parameters.toMap())

/**
 * The envelope of [value] for a send. An envelope that already has a fallback
 * keeps it, and `fallback` is not called; otherwise `fallback(value)` supplies
 * it. A failed step, or an envelope of another type than the codec's, is
 * `CodecEncodeFailed`.
 */
internal fun <T : Any> encodeForSend(
    codec: ContentCodec<T>,
    value: T,
    type: ContentTypeId = codecType(codec),
): EncodedContent {
    val encoded = codecStep("encode") { codec.encode(value).snapshot() }
    // implements: CTYPE-007
    // The envelope type is the codec's type, so a codec's push hook cannot
    // steer catalogue dispatch.
    if (encoded.type != type) {
        throw codecEncodeFailed("encode", "the envelope type differs from the codec type")
    }
    // implements: CTYPE-003
    // An empty authority or type ID fails here, before the send, not in the
    // binding.
    if (encoded.type.authorityId.isEmpty() || encoded.type.typeId.isEmpty()) {
        throw codecEncodeFailed("encode", "the envelope type has an empty authority or type ID")
    }
    if (encoded.fallback != null || usesRustStandardFallback(codec)) return encoded
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
    type: ContentTypeId = codecType(codec),
): SendOptions? {
    if (options?.shouldPush != null || isCatalogueContentType(type)) return options
    val push = codecStep("shouldPush") { codec.shouldPush(value) }
    return (options ?: SendOptions()).copy(shouldPush = push)
}

/**
 * The envelope of [value] for a reply. The caller's cancellation is checked
 * before the codec steps run and again after them, because a slow synchronous
 * step cannot see a cancellation while it runs.
 */
internal suspend fun <T : Any> replyEnvelope(
    codec: ContentCodec<T>,
    value: T,
): EncodedContent {
    currentCoroutineContext().ensureActive()
    val encoded = encodeForSend(codec, value)
    currentCoroutineContext().ensureActive()
    return encoded
}

/**
 * The envelope and options of [value] for a send or prepare. The codec's
 * type is read once. The caller's cancellation is checked before the codec
 * steps run and again after them, before the send starts.
 */
private suspend fun <T : Any> sendParts(
    codec: ContentCodec<T>,
    value: T,
    options: SendOptions?,
): Pair<EncodedContent, SendOptions?> {
    currentCoroutineContext().ensureActive()
    val type = codecType(codec)
    val encoded = encodeForSend(codec, value, type)
    val sendOptions = optionsForSend(codec, value, options, type)
    currentCoroutineContext().ensureActive()
    return encoded to sendOptions
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

// Standard encoders already apply their canonical fallback, including null.
private fun usesRustStandardFallback(codec: Any): Boolean =
    codec is TextCodec ||
        codec is MarkdownCodec ||
        codec is ReadReceiptCodec ||
        codec is ReactionV2Codec ||
        codec is AttachmentCodec ||
        codec is RemoteAttachmentCodec ||
        codec is MultiRemoteAttachmentCodec ||
        codec is TransactionReferenceCodec ||
        codec is WalletSendCallsCodec ||
        codec is ActionsCodec ||
        codec is IntentCodec ||
        codec is ReplyCodec ||
        codec is GroupUpdatedCodec ||
        codec is DeleteMessageCodec ||
        codec is LeaveRequestCodec

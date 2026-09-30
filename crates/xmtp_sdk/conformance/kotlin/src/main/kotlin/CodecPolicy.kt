import uniffi.xmtp_sdk.*

// The typed codec send policy (Ref Public surface, Host codecs; P9 and P10).

/** A standard codec's bytes equal Rust's, and a decode round trip keeps them. */
internal fun <T : Any> matchesRust(
    codec: ContentCodec<T>,
    value: T,
    expected: EncodedContent,
): Boolean {
    val encoded = codec.encode(value)
    return sameEncoded(encoded, expected) && sameEncoded(codec.encode(codec.decode(encoded)), expected)
}

private val noteType = ContentTypeId("example.org", "note", 1u, 0u)

/** A note codec. Each step can fail, return its own fallback, or change type. */
private class NoteCodec(
    val failEncode: Boolean = false,
    val failFallback: Boolean = false,
    val failPush: Boolean = false,
    val ownFallback: String? = null,
    val envelopeType: ContentTypeId = noteType,
    val push: Boolean = true,
) : ContentCodec<String> {
    override val type = noteType

    override fun encode(value: String): EncodedContent {
        if (failEncode) throw IllegalStateException("encode must not run")
        return EncodedContent(envelopeType, emptyMap(), ownFallback, value.toByteArray())
    }

    override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()

    override fun fallback(value: String): String {
        if (failFallback) throw IllegalStateException("fallback must not run")
        return "a note: $value"
    }

    override fun shouldPush(value: String): Boolean {
        if (failPush) throw IllegalStateException("shouldPush must not run")
        return push
    }
}

/** A codec of a catalogue type whose push hook must not run. */
private class CatalogueTextCodec : ContentCodec<String> {
    override val type = TextCodec().type

    override fun encode(value: String) = TextCodec().encode(value)

    override fun decode(encoded: EncodedContent) = TextCodec().decode(encoded)

    override fun shouldPush(value: String): Boolean = throw IllegalStateException("shouldPush for a catalogue type")
}

/** A Group with no Rust object that records the options each send receives. */
private class RecordingGroup : Group(NoHandle) {
    val sent = mutableListOf<SendOptions?>()

    override suspend fun send(
        encoded: EncodedContent,
        options: SendOptions?,
    ): MessageId {
        sent += options
        return "recorded"
    }

    override suspend fun prepareMessage(
        encoded: EncodedContent,
        options: SendOptions?,
    ): MessageId = send(encoded, options)
}

/** The push hook result reaches the send options, unless an option or the catalogue decides. */
private suspend fun checkPushOptions() {
    val group = RecordingGroup()
    group.send(NoteCodec(push = false), "quiet")
    group.prepareMessage(NoteCodec(push = true), "loud")
    group.send(NoteCodec(failPush = true), "explicit", SendOptions(shouldPush = false, optimistic = true))
    group.send(CatalogueTextCodec(), "catalogue")
    val pushes = group.sent.map { it?.shouldPush }
    check(pushes == listOf(false, true, false, null)) { "the send options have the wrong push values: $pushes" }
    check(group.sent[2]?.optimistic == true) { "an explicit option lost its other fields" }
}

private fun envelope(message: Message?): EncodedContent? =
    when (val content = message?.content) {
        is SDKMessageContent.Custom -> content.encoded
        is SDKMessageContent.Unknown -> content.encoded
        else -> null
    }

private suspend fun expectCodecEncodeFailed(
    what: String,
    attempt: suspend () -> Unit,
) {
    val error = runCatching { attempt() }.exceptionOrNull()
    val details = (error as? XmtpException.CodecEncodeFailed)?.v1
    check(details != null && details.code == "CodecEncodeFailed" && !details.retryable) {
        "$what did not fail with CodecEncodeFailed: $error"
    }
}

// verifies: CTYPE-017, CTYPE-021
internal suspend fun checkCodecPolicy(
    group: Group,
    receiver: SDKClient,
) {
    checkPushOptions()

    suspend fun stored(id: MessageId): Message? = group.messages(null).firstOrNull { it.id == id }

    // A typed send fills the fallback; an envelope's own fallback is kept.
    val sentId = group.send(NoteCodec(), "typed send")
    val sent = stored(sentId)
    check(envelope(sent)?.fallback == "a note: typed send") { "a typed send did not fill the fallback" }
    val keptId = group.send(NoteCodec(failFallback = true, ownFallback = "own"), "kept")
    check(envelope(stored(keptId))?.fallback == "own") { "an envelope fallback was replaced" }

    // An explicit shouldPush and a catalogue type skip the push hook.
    group.send(NoteCodec(failPush = true), "explicit", SendOptions(shouldPush = false))
    group.send(CatalogueTextCodec(), "catalogue text")

    // prepareMessage takes the codec form and stores an unpublished item.
    val preparedId = group.prepareMessage(NoteCodec(), "prepared")
    check(stored(preparedId)?.data?.deliveryStatus == DeliveryStatus.UNPUBLISHED) {
        "a typed prepareMessage did not store an unpublished item"
    }
    group.publishMessage(preparedId)

    // A typed reply fills the nested fallback and keeps the reply's push.
    val parent = checkNotNull(sent) { "the typed send was not stored" }
    val replyId = parent.reply(NoteCodec(failPush = true), "typed reply")
    val nested = (stored(replyId)?.replyContent as? SDKReplyContent.Unknown)?.encoded
    check(nested?.fallback == "a note: typed reply") { "a typed reply did not fill the nested fallback" }

    // A receiver without the codec keeps the envelope and its fallback.
    receiver.conversations().syncAll(null)
    val received = receiver.conversations().getMessageById(sentId)?.content
    check(received is SDKMessageContent.Unknown && received.encoded.fallback == "a note: typed send") {
        "a receiver without the codec lost the envelope"
    }

    // A failed step makes no publish attempt, on send, prepare, and reply.
    val before = group.messages(null).size
    for (codec in listOf(
        NoteCodec(failEncode = true),
        NoteCodec(failFallback = true),
        NoteCodec(failPush = true),
        NoteCodec(envelopeType = TextCodec().type),
    )) {
        expectCodecEncodeFailed("send") { group.send(codec, "x") }
        expectCodecEncodeFailed("prepareMessage") { group.prepareMessage(codec, "x") }
    }
    expectCodecEncodeFailed("reply") { parent.reply(NoteCodec(failEncode = true), "x") }
    check(group.messages(null).size == before) { "a failed codec step made a publish attempt" }
}

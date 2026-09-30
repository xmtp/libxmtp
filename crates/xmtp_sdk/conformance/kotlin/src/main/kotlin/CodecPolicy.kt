import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import uniffi.xmtp_sdk.*

// The typed codec send policy (Ref Public surface, Host codecs; P9 and P10).

/**
 * A standard codec's bytes equal Rust's, decoding them gives back an equal
 * value, and re-encoding that value gives the same bytes.
 */
internal fun <T : Any> matchesRust(
    codec: ContentCodec<T>,
    value: T,
    expected: EncodedContent,
): Boolean {
    val encoded = codec.encode(value)
    val decoded = codec.decode(encoded)
    // The generated records compare every field, byte arrays by content.
    return sameEncoded(encoded, expected) && decoded == value && sameEncoded(codec.encode(decoded), expected)
}

private val noteType = ContentTypeId("example.org", "note", 1u, 0u)
private val emptyType = ContentTypeId("", "note", 1u, 0u)

/** A note codec. Each step can fail, return its own fallback, or change type. */
private class NoteCodec(
    val failEncode: Boolean = false,
    val cancelEncode: Boolean = false,
    val todoEncode: Boolean = false,
    val unreadableEncode: Boolean = false,
    val failFallback: Boolean = false,
    val failPush: Boolean = false,
    val ownFallback: String? = null,
    val envelopeType: ContentTypeId = noteType,
    val push: Boolean = true,
    val codecType: ContentTypeId = noteType,
) : ContentCodec<String> {
    override val type = codecType

    override fun encode(value: String): EncodedContent {
        if (failEncode) throw IllegalStateException("encode must not run")
        // A codec's own CancellationException is a codec failure, not a
        // cancellation of the caller.
        if (cancelEncode) throw java.util.concurrent.CancellationException("encode cancelled")
        if (todoEncode) TODO("encode")
        if (unreadableEncode) throw UnreadableFailure()
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
    val envelopes = mutableListOf<EncodedContent>()

    override suspend fun send(
        encoded: EncodedContent,
        options: SendOptions?,
    ): MessageId {
        sent += options
        envelopes += encoded
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

/** A codec that records whether any step ran. */
private class RecordingCodec : ContentCodec<String> {
    @Volatile var called = false
    override val type = noteType

    override fun encode(value: String): EncodedContent {
        called = true
        return EncodedContent(noteType, emptyMap(), null, value.toByteArray())
    }

    override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()

    override fun fallback(value: String): String? {
        called = true
        return null
    }

    override fun shouldPush(value: String): Boolean {
        called = true
        return true
    }
}

/** A failure whose message and text cannot be read. */
private class UnreadableFailure : RuntimeException() {
    override val message: String
        get() = throw IllegalStateException("message")

    override fun toString(): String = throw IllegalStateException("toString")
}

/**
 * A codec that counts reads of its type, can throw from its type, can run an
 * action inside `encode`, and can change the envelope it returned from inside
 * its fallback hook.
 */
private class ProbeCodec(
    val throwingType: Boolean = false,
    val onEncode: () -> Unit = {},
    val changeEnvelope: Boolean = false,
) : ContentCodec<String> {
    var typeReads = 0
    var encoded = false
    private var kept: EncodedContent? = null

    override val type: ContentTypeId
        get() {
            typeReads += 1
            if (throwingType) throw IllegalStateException("type getter")
            return noteType
        }

    override fun encode(value: String): EncodedContent {
        encoded = true
        onEncode()
        return EncodedContent(noteType.copy(), emptyMap(), null, value.toByteArray()).also { kept = it }
    }

    override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()

    override fun fallback(value: String): String {
        // The generated record has `var` fields, so a codec can change the
        // envelope object that it returned.
        if (changeEnvelope) kept?.type = TextCodec().type
        return "a note"
    }
}

/**
 * The codec's type is read once per send and a throwing type is
 * CodecEncodeFailed. The policy checks its own copy of the envelope. A caller
 * cancelled during a codec step stops before the send.
 */
private suspend fun checkProbes() {
    val group = RecordingGroup()
    val counted = ProbeCodec()
    group.send(counted, "counted")
    check(counted.typeReads == 1) { "a typed send read the codec type ${counted.typeReads} times" }
    expectCodecEncodeFailed("a throwing type") { group.send(ProbeCodec(throwingType = true), "x") }

    val changing = ProbeCodec(changeEnvelope = true)
    group.send(changing, "changed")
    check(group.envelopes.last().type == noteType) { "a hook changed the checked envelope type" }

    var error: Throwable? = null
    val cancelling = ProbeCodec()
    val sends = group.sent.size
    coroutineScope {
        launch {
            val job = currentCoroutineContext().job
            // The caller is cancelled while a synchronous codec step runs.
            val codec = ProbeCodec(onEncode = { job.cancel() })
            error = runCatching { group.send(codec, "cancelled") }.exceptionOrNull()
            cancelling.encoded = codec.encoded
        }
    }
    check(cancelling.encoded) { "the cancelling codec did not run" }
    check(error is kotlinx.coroutines.CancellationException) { "a send cancelled in a codec step did not stop: $error" }
    check(group.sent.size == sends) { "a send cancelled in a codec step reached the send" }
}

/** A cancelled caller stops before any codec step runs, with no send. */
private suspend fun checkCallerCancellation() {
    val group = RecordingGroup()
    val codec = RecordingCodec()
    var error: Throwable? = null
    coroutineScope {
        launch {
            // The job is cancelled but still runs until it next checks.
            currentCoroutineContext().job.cancel()
            error = runCatching { group.send(codec, "cancelled") }.exceptionOrNull()
        }
    }
    check(error is kotlinx.coroutines.CancellationException) { "a cancelled send did not stop: $error" }
    check(!codec.called) { "a codec step ran for a cancelled caller" }
    check(group.sent.isEmpty()) { "a cancelled caller sent" }
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

private suspend fun Group.stored(id: MessageId): Message? = messages(null).firstOrNull { it.id == id }

// verifies: CTYPE-017, CTYPE-021, SEND-021

/**
 * custom_codec_policy_and_isolation: a typed send, prepare, and reply apply
 * the codec's fallback and push hooks, an explicit push value wins, and a
 * client without the codec keeps the envelope. Returns the typed send, a
 * parent for the failure checks.
 */
internal suspend fun customCodecPolicyAndIsolation(
    group: Group,
    receiver: SDKClient,
): Message {
    checkPushOptions()

    // A typed send fills the fallback.
    val sentId = group.send(NoteCodec(), "typed send")
    val sent = group.stored(sentId)
    check(envelope(sent)?.fallback == "a note: typed send") { "a typed send did not fill the fallback" }

    // prepareMessage takes the codec form and stores an unpublished item.
    val preparedId = group.prepareMessage(NoteCodec(), "prepared")
    check(group.stored(preparedId)?.data?.deliveryStatus == DeliveryStatus.UNPUBLISHED) {
        "a typed prepareMessage did not store an unpublished item"
    }
    group.publishMessage(preparedId)

    // A typed reply fills the nested fallback and keeps the reply's push.
    val parent = checkNotNull(sent) { "the typed send was not stored" }
    val replyId = parent.reply(NoteCodec(failPush = true), "typed reply")
    val nested = (group.stored(replyId)?.replyContent as? SDKReplyContent.Unknown)?.encoded
    check(nested?.fallback == "a note: typed reply") { "a typed reply did not fill the nested fallback" }

    // A receiver without the codec keeps the envelope and its fallback.
    receiver.conversations().syncAll(null)
    val received = receiver.conversations().getMessageById(sentId)?.content
    check(received is SDKMessageContent.Unknown && received.encoded.fallback == "a note: typed send") {
        "a receiver without the codec lost the envelope"
    }
    return parent
}

// verifies: CTYPE-003, CTYPE-007, GMOD-035

/**
 * codec_policy_failure_never_publishes: a skipped hook is not called, and a
 * failed encode, fallback, or shouldPush step, or an envelope of another type,
 * is CodecEncodeFailed with no publish attempt.
 */
internal suspend fun codecPolicyFailureNeverPublishes(
    group: Group,
    parent: Message,
) {
    checkCallerCancellation()
    checkProbes()
    // An envelope's own fallback is kept, and its fallback hook is not called.
    val keptId = group.send(NoteCodec(failFallback = true, ownFallback = "own"), "kept")
    check(envelope(group.stored(keptId))?.fallback == "own") { "an envelope fallback was replaced" }
    // An explicit shouldPush and a catalogue type skip the push hook.
    group.send(NoteCodec(failPush = true), "explicit", SendOptions(shouldPush = false))
    group.send(CatalogueTextCodec(), "catalogue text")

    // A failed step makes no publish attempt, on send, prepare, and reply.
    val before = group.messages(null).size
    for (codec in listOf(
        NoteCodec(failEncode = true),
        NoteCodec(cancelEncode = true),
        NoteCodec(todoEncode = true),
        NoteCodec(unreadableEncode = true),
        NoteCodec(failFallback = true),
        NoteCodec(failPush = true),
        NoteCodec(envelopeType = TextCodec().type),
        // The codec and its envelope agree; only the empty identifier fails.
        NoteCodec(envelopeType = emptyType, codecType = emptyType),
    )) {
        expectCodecEncodeFailed("send") { group.send(codec, "x") }
        expectCodecEncodeFailed("prepareMessage") { group.prepareMessage(codec, "x") }
    }
    expectCodecEncodeFailed("reply") { parent.reply(NoteCodec(failEncode = true), "x") }
    // Known gap, waiting for an owner decision: the reaction, reply, and
    // delete-message codecs take the whole StandardContent, so the compiler
    // accepts another variant. The send fails at run time instead.
    for (codec in listOf(ReactionV2Codec(), ReplyCodec(), DeleteMessageCodec())) {
        expectCodecEncodeFailed("${codec.javaClass.simpleName} send") {
            group.send(codec, StandardContent.Text("x"))
        }
    }
    // A typed send of a transcript type reaches core's reserved-type guard on
    // send and prepare, and stores nothing.
    val groupUpdated =
        sdkConformanceStandardSamples()
            .map { it.value }
            .filterIsInstance<StandardContent.GroupUpdated>()
            .single()
            .v1
    for (attempt in listOf<suspend () -> Unit>(
        { group.send(GroupUpdatedCodec(), groupUpdated) },
        { group.prepareMessage(GroupUpdatedCodec(), groupUpdated) },
    )) {
        val refused = runCatching { attempt() }.exceptionOrNull()
        val details = (refused as? XmtpException.InvalidInput)?.v1
        check(details?.code == "ReservedTranscriptContentType" && !details.retryable) {
            "a typed GroupUpdatedCodec send was not refused: $refused"
        }
    }
    check(group.messages(null).size == before) { "a failed codec step made a publish attempt" }
}

package uniffi.xmtp_sdk

import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.xmtp.android.library.deliveryTestMessage
import org.xmtp.android.library.testSDKClient
import java.util.concurrent.atomic.AtomicLong

// The typed codec send policy in SDKCodecPolicy.kt and Message.reply(codec, value)
// in SDKTypes.kt are hand-written Kotlin. TypedCodecSendTest checks the call
// routing of each typed form; this test checks the policy. Rust owns the
// catalogue push defaults and the reserved transcript types:
// xmtp_sdk/src/tests/content_decode.rs::encoded_sends_use_catalogue_push_defaults_and_explicit_override,
// xmtp_mls/src/groups/mls_sync/publish/tests/transcript.rs.
class CodecPolicyTest {
    private val noteType = ContentTypeId("example.org", "note", 1u, 0u)
    private val emptyType = ContentTypeId("", "note", 1u, 0u)

    /** A note codec. Each step can fail, return its own fallback, or change type. */
    private inner class NoteCodec(
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

    /** A failure whose message and text cannot be read. */
    private class UnreadableFailure : RuntimeException() {
        override val message: String
            get() = throw IllegalStateException("message")

        override fun toString(): String = throw IllegalStateException("toString")
    }

    /** A failure whose text is a fatal VM error. */
    private class FatalTextFailure : RuntimeException() {
        override val message: String? get() = null

        override fun toString(): String = throw InternalError("fatal while describing a codec failure")
    }

    /** A codec that records whether any step ran. */
    private inner class RecordingCodec : ContentCodec<String> {
        @Volatile var called = false
        override val type = noteType

        override fun encode(value: String): EncodedContent {
            called = true
            return EncodedContent(noteType, emptyMap(), null, value.toByteArray())
        }

        override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()

        override fun shouldPush(value: String): Boolean {
            called = true
            return true
        }
    }

    /**
     * Counts reads of its type, can throw from its type, can run an action
     * inside `encode`, and can change the envelope it returned from inside its
     * fallback hook.
     */
    private inner class ProbeCodec(
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

    /** A Group with no Rust object that records each send. */
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

    /** A native client whose conversations record each reply. */
    private class ReplyClient : Client(NoHandle) {
        val key = keys.getAndIncrement().toULong()
        val replies = mutableListOf<Triple<MessageId, EncodedContent, SendOptions?>>()
        private val conversations =
            object : Conversations(NoHandle) {
                override suspend fun replyToMessage(
                    id: MessageId,
                    content: EncodedContent,
                    options: SendOptions?,
                ): MessageId {
                    replies += Triple(id, content, options)
                    return "reply"
                }
            }

        override fun clientKey(): ULong = key

        override fun conversations(): Conversations = conversations

        companion object {
            val keys = AtomicLong(1_000)
        }
    }

    private suspend fun expectCodecEncodeFailed(
        what: String,
        attempt: suspend () -> Unit,
    ) {
        val error = runCatching { attempt() }.exceptionOrNull()
        val details = (error as? XmtpException.CodecEncodeFailed)?.v1
        assertTrue("$what did not fail with CodecEncodeFailed: $error", details != null)
        assertEquals("CodecEncodeFailed", details!!.code)
        assertEquals(ErrorCategory.CALLBACK, details.category)
        assertFalse(details.retryable)
    }

    // verifies: CTYPE-017, CTYPE-021
    @Test
    fun typedSendFillsTheFallbackAndKeepsAnEnvelopeFallback() =
        runBlocking {
            val group = RecordingGroup()
            group.send(NoteCodec(), "typed send")
            group.prepareMessage(NoteCodec(), "prepared")
            assertEquals(listOf("a note: typed send", "a note: prepared"), group.envelopes.map { it.fallback })
            // An envelope's own fallback is kept, and its fallback hook is not called.
            group.send(NoteCodec(failFallback = true, ownFallback = "own"), "kept")
            assertEquals("own", group.envelopes.last().fallback)
        }

    // verifies: SEND-021
    @Test
    fun pushHookFeedsTheSendOptionsUnlessAnOptionOrTheCatalogueDecides() =
        runBlocking {
            val group = RecordingGroup()
            group.send(NoteCodec(push = false), "quiet")
            group.prepareMessage(NoteCodec(push = true), "loud")
            group.send(NoteCodec(failPush = true), "explicit", SendOptions(shouldPush = false, optimistic = true))
            group.send(CatalogueTextCodec(), "catalogue")
            assertEquals(listOf(false, true, false, null), group.sent.map { it?.shouldPush })
            assertEquals("An explicit option lost its other fields", true, group.sent[2]?.optimistic)
        }

    // verifies: CTYPE-003, CTYPE-007
    @Test
    fun failedCodecStepNeverSends() =
        runBlocking {
            val group = RecordingGroup()
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
            assertTrue("A failed codec step reached the send", group.sent.isEmpty())
            // A fatal VM error while describing a failure is not swallowed or replaced.
            val fatal =
                object : ContentCodec<String> {
                    override val type = noteType

                    override fun encode(value: String): EncodedContent = throw FatalTextFailure()

                    override fun decode(encoded: EncodedContent) = encoded.content.decodeToString()
                }
            val error = runCatching { group.send(fatal, "x") }.exceptionOrNull()
            assertTrue("A fatal error while describing a failure did not propagate: $error", error is InternalError)
            assertTrue(group.sent.isEmpty())
        }

    @Test
    fun typeIsReadOnceAndHooksCannotChangeTheCheckedEnvelope() =
        runBlocking {
            val group = RecordingGroup()
            val counted = ProbeCodec()
            group.send(counted, "counted")
            assertEquals("A typed send read the codec type more than once", 1, counted.typeReads)
            expectCodecEncodeFailed("a throwing type") { group.send(ProbeCodec(throwingType = true), "x") }
            group.send(ProbeCodec(changeEnvelope = true), "changed")
            assertEquals("A hook changed the checked envelope type", noteType, group.envelopes.last().type)
        }

    @Test
    fun callerCancellationStopsTheSend() =
        runBlocking {
            val group = RecordingGroup()
            // A caller cancelled before the send runs no codec step.
            val recording = RecordingCodec()
            var error: Throwable? = null
            coroutineScope {
                launch {
                    currentCoroutineContext().job.cancel()
                    error = runCatching { group.send(recording, "cancelled") }.exceptionOrNull()
                }
            }
            assertTrue("A cancelled send did not stop: $error", error is kotlinx.coroutines.CancellationException)
            assertFalse("A codec step ran for a cancelled caller", recording.called)
            // A caller cancelled while a synchronous codec step runs stops before the send.
            var probe: ProbeCodec? = null
            coroutineScope {
                launch {
                    val job = currentCoroutineContext().job
                    val codec = ProbeCodec(onEncode = { job.cancel() }).also { probe = it }
                    error = runCatching { group.send(codec, "cancelled") }.exceptionOrNull()
                }
            }
            assertTrue("The cancelling codec did not run", probe!!.encoded)
            assertTrue(
                "A send cancelled in a codec step did not stop: $error",
                error is kotlinx.coroutines.CancellationException,
            )
            assertTrue("A cancelled caller sent", group.sent.isEmpty())
        }

    // verifies: CTYPE-007
    @Test
    fun typedReplyFillsTheNestedFallbackAndKeepsTheReplyPushDefault() =
        runBlocking {
            val raw = ReplyClient()
            val client = testSDKClient(raw)
            ClientRegistry.register(client)
            try {
                val parent = deliveryTestMessage(clientKey = raw.key)
                parent.reply(NoteCodec(failPush = true), "typed reply")
                val (id, nested, options) = raw.replies.single()
                assertEquals(parent.id, id)
                assertEquals("a note: typed reply", nested.fallback)
                assertNull("A typed reply used the codec push hook", options)
                val explicit = SendOptions(shouldPush = false)
                parent.reply(NoteCodec(), "explicit", explicit)
                assertEquals(explicit, raw.replies.last().third)
                expectCodecEncodeFailed("reply") { parent.reply(NoteCodec(failEncode = true), "x") }
                var error: Throwable? = null
                coroutineScope {
                    launch {
                        currentCoroutineContext().job.cancel()
                        error = runCatching { parent.reply(NoteCodec(), "cancelled") }.exceptionOrNull()
                    }
                }
                assertTrue(
                    "A cancelled typed reply did not stop: $error",
                    error is kotlinx.coroutines.CancellationException,
                )
                assertEquals("A failed or cancelled typed reply was sent", 2, raw.replies.size)
            } finally {
                ClientRegistry.remove(client)
            }
        }
}

package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// The typed codec forms of send and prepareMessage in SDKCodecPolicy.kt are
// hand-written Kotlin. Each one must encode with the codec, call the envelope
// form of the same method on the same conversation, and keep the caller's
// options. The fake Group and Dm record each envelope call.
class TypedCodecSendTest {
    private data class Call(
        val method: String,
        val encoded: EncodedContent,
        val options: SendOptions?,
    )

    private class Recorder {
        val calls = mutableListOf<Call>()

        fun record(
            method: String,
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId {
            calls += Call(method, encoded, options)
            return method
        }
    }

    private class RecordingGroup(
        val recorder: Recorder = Recorder(),
    ) : Group(NoHandle) {
        override suspend fun send(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = recorder.record("group.send", encoded, options)

        override suspend fun prepareMessage(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = recorder.record("group.prepareMessage", encoded, options)
    }

    private class RecordingDm(
        val recorder: Recorder = Recorder(),
    ) : Dm(NoHandle) {
        override suspend fun send(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = recorder.record("dm.send", encoded, options)

        override suspend fun prepareMessage(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = recorder.record("dm.prepareMessage", encoded, options)
    }

    // A custom codec. Its hooks supply the fallback and the push default.
    private class NoteCodec : ContentCodec<String> {
        override val type = ContentTypeId("example.com", "note", 1u, 0u)

        override fun encode(value: String) = EncodedContent(type, content = value.toByteArray())

        override fun decode(encoded: EncodedContent) = encoded.content.toString(Charsets.UTF_8)

        override fun fallback(value: String) = "note: $value"

        override fun shouldPush(value: String) = false
    }

    private val explicit = SendOptions(shouldPush = true, optimistic = true)

    private fun Recorder.checkSingleTextCall(
        method: String,
        text: String,
    ) {
        val call = calls.single()
        assertEquals(method, call.method)
        assertEquals(TextCodec().type, call.encoded.type)
        assertEquals(text, TextCodec().decode(call.encoded))
        assertEquals(explicit, call.options)
    }

    @Test
    fun groupFormsEncodeAndCallTheirOwnEnvelopeMethod() =
        runBlocking {
            val send = RecordingGroup()
            assertEquals("group.send", send.send(TextCodec(), "sent", explicit))
            send.recorder.checkSingleTextCall("group.send", "sent")

            val prepare = RecordingGroup()
            assertEquals("group.prepareMessage", prepare.prepareMessage(TextCodec(), "prepared", explicit))
            prepare.recorder.checkSingleTextCall("group.prepareMessage", "prepared")
        }

    @Test
    fun dmFormsEncodeAndCallTheirOwnEnvelopeMethod() =
        runBlocking {
            val send = RecordingDm()
            assertEquals("dm.send", send.send(TextCodec(), "sent", explicit))
            send.recorder.checkSingleTextCall("dm.send", "sent")

            val prepare = RecordingDm()
            assertEquals("dm.prepareMessage", prepare.prepareMessage(TextCodec(), "prepared", explicit))
            prepare.recorder.checkSingleTextCall("dm.prepareMessage", "prepared")
        }

    @Test
    fun conversationFormsReachTheWrappedGroupOrDm() =
        runBlocking {
            val group = RecordingGroup()
            Conversation.Group(group).send(TextCodec(), "group sent", explicit)
            Conversation.Group(group).prepareMessage(TextCodec(), "group prepared", explicit)
            assertEquals(listOf("group.send", "group.prepareMessage"), group.recorder.calls.map { it.method })
            assertEquals(
                listOf("group sent", "group prepared"),
                group.recorder.calls.map { TextCodec().decode(it.encoded) },
            )

            val dm = RecordingDm()
            Conversation.Dm(dm).send(TextCodec(), "dm sent", explicit)
            Conversation.Dm(dm).prepareMessage(TextCodec(), "dm prepared", explicit)
            assertEquals(listOf("dm.send", "dm.prepareMessage"), dm.recorder.calls.map { it.method })
            assertEquals(
                listOf("dm sent", "dm prepared"),
                dm.recorder.calls.map { TextCodec().decode(it.encoded) },
            )
            assertEquals(List(4) { explicit }, (group.recorder.calls + dm.recorder.calls).map { it.options })
        }

    @Test
    fun customCodecHooksSetTheFallbackAndPushOfASend() =
        runBlocking {
            val group = RecordingGroup()
            group.send(NoteCodec(), "hello")
            val call = group.recorder.calls.single()
            assertEquals("note: hello", call.encoded.fallback)
            assertEquals(SendOptions(shouldPush = false), call.options)
            // A catalogue codec keeps the caller's null options; Rust applies the default.
            group.send(TextCodec(), "catalogue")
            assertNull(
                group.recorder.calls
                    .last()
                    .options,
            )
        }
}

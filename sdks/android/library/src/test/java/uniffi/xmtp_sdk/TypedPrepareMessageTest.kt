package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

// The typed codec forms of prepareMessage in SDKCodecPolicy.kt are hand-written
// Kotlin. Each one must encode with the codec and call the envelope form of
// prepareMessage, not send, and keep the caller's options.
class TypedPrepareMessageTest {
    private class RecordingGroup : Group(NoHandle) {
        val calls = mutableListOf<Pair<String, EncodedContent>>()
        val options = mutableListOf<SendOptions?>()

        override suspend fun send(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = record("send", encoded, options)

        override suspend fun prepareMessage(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = record("prepareMessage", encoded, options)

        private fun record(
            method: String,
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId {
            calls += method to encoded
            this.options += options
            return method
        }
    }

    private class RecordingDm : Dm(NoHandle) {
        val calls = mutableListOf<Pair<String, EncodedContent>>()
        val options = mutableListOf<SendOptions?>()

        override suspend fun send(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = record("send", encoded, options)

        override suspend fun prepareMessage(
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId = record("prepareMessage", encoded, options)

        private fun record(
            method: String,
            encoded: EncodedContent,
            options: SendOptions?,
        ): MessageId {
            calls += method to encoded
            this.options += options
            return method
        }
    }

    private val explicit = SendOptions(shouldPush = false, optimistic = true)

    @Test
    fun groupPrepareMessageEncodesAndPrepares() =
        runBlocking {
            val group = RecordingGroup()
            assertEquals("prepareMessage", group.prepareMessage(TextCodec(), "prepared", explicit))
            val (method, encoded) = group.calls.single()
            assertEquals("prepareMessage", method)
            assertEquals(TextCodec().type, encoded.type)
            assertEquals("prepared", TextCodec().decode(encoded))
            assertEquals(explicit, group.options.single())
        }

    @Test
    fun dmPrepareMessageEncodesAndPrepares() =
        runBlocking {
            val dm = RecordingDm()
            assertEquals("prepareMessage", dm.prepareMessage(TextCodec(), "prepared", explicit))
            val (method, encoded) = dm.calls.single()
            assertEquals("prepareMessage", method)
            assertEquals(TextCodec().type, encoded.type)
            assertEquals("prepared", TextCodec().decode(encoded))
            assertEquals(explicit, dm.options.single())
        }
}

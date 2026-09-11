package org.xmtp.android.library

import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.xmtp.android.library.libxmtp.DecodedMessage
import uniffi.xmtpv3.FfiConversationMessageKind

/** Keeps callback reads and writes under one lock. */
internal class StreamTestMessages {
    private val messages = mutableListOf<DecodedMessage>()

    fun add(message: DecodedMessage) {
        synchronized(messages) { messages.add(message) }
    }

    fun snapshot(): List<DecodedMessage> = synchronized(messages) { messages.toList() }

    /** Waits for the exact application ID and body order. Membership rows remain in the stream. */
    suspend fun awaitApplications(expected: List<Pair<String, String>>) {
        withTimeout(30_000) {
            while (applications().size < expected.size) {
                delay(10)
            }
        }
        assertEquals(expected, applications())
    }

    private fun applications(): List<Pair<String, String>> =
        snapshot()
            .filter { it.kind == FfiConversationMessageKind.APPLICATION }
            .map { it.id to it.body }

    /** Compares every decoded row in database delivery order and rejects duplicate IDs. */
    suspend fun awaitHistory(expected: List<DecodedMessage>) {
        withTimeout(30_000) {
            while (snapshot().size < expected.size) {
                delay(10)
            }
        }
        assertHistory(expected)
    }

    fun assertHistory(expected: List<DecodedMessage>) {
        val actual = snapshot()
        assertEquals(actual.size, actual.map { it.id }.toSet().size)
        assertEquals(expected.map { it.id }, actual.map { it.id })
        expected.zip(actual).forEach { (stored, streamed) ->
            assertEquals(stored.conversationId, streamed.conversationId)
            assertEquals(stored.kind, streamed.kind)
            assertEquals(stored.encodedContent, streamed.encodedContent)
        }
    }
}

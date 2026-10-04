package org.xmtp.android.library

import kotlinx.coroutines.delay
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import uniffi.xmtp_sdk.*

internal fun messageText(message: Message): String =
    ((message.content as SDKMessageContent.Standard).value as MessageContent.Text).v1

/** Reads and writes use the same lock. */
internal class StreamTestMessages {
    private val messages = mutableListOf<Message>()

    fun add(message: Message) {
        synchronized(messages) { messages.add(message) }
    }

    fun snapshot(): List<Message> = synchronized(messages) { messages.toList() }

    suspend fun awaitApplications(expected: List<Pair<String, String>>) {
        withTimeout(30_000) {
            while (applications().size < expected.size) delay(10)
        }
        assertEquals(expected, applications())
    }

    private fun applications(): List<Pair<String, String>> =
        snapshot()
            .filter { it.kind == MessageKind.APPLICATION }
            .map { it.id to messageText(it) }

    suspend fun awaitHistory(expected: List<Message>) {
        withTimeout(30_000) {
            while (snapshot().size < expected.size) delay(10)
        }
        assertHistory(expected)
    }

    fun assertHistory(expected: List<Message>) {
        val actual = snapshot()
        assertEquals(actual.size, actual.map { it.id }.toSet().size)
        assertEquals(expected.map { it.id }, actual.map { it.id })
        expected.zip(actual).forEach { (stored, streamed) ->
            assertEquals(stored.conversationId, streamed.conversationId)
            assertEquals(stored.kind, streamed.kind)
            assertEquals(stored.encoded, streamed.encoded)
            assertArrayEquals(stored.rawBytes, streamed.rawBytes)
        }
    }
}

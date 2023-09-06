package org.xmtp.android.library

import com.google.protobuf.kotlin.toByteStringUtf8
import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.Envelope

class PreparedMessageTest {

    @Test
    fun testSerializing() {
        val original = PreparedMessage(
            listOf(
                Envelope.newBuilder().apply {
                    contentTopic = "topic1"
                    timestampNs = 1234
                    message = "abc123".toByteStringUtf8()
                }.build(),
                Envelope.newBuilder().apply {
                    contentTopic = "topic2"
                    timestampNs = 5678
                    message = "def456".toByteStringUtf8()
                }.build(),
            )
        )
        val serialized = original.toSerializedData()
        val unserialized = PreparedMessage.fromSerializedData(serialized)
        assertEquals(original, unserialized)
    }
}

package org.xmtp.android.example.messenger.attachments

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.SendDraftRef
import org.xmtp.android.example.messenger.SendPhase
import uniffi.xmtp_sdk.RemoteAttachment

class AttachmentBoundaryTest {
    @Test fun copyEnforcesActualBytesAndNeverRequestsMoreThan64KiB() = runBlocking {
        val bytes = ByteArray(128 * 1024 + 1) { (it % 251).toByte() }
        val stream = object : ByteArrayInputStream(bytes) {
            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                assertTrue(length <= 64 * 1024)
                return super.read(buffer, offset, length)
            }
        }
        val output = ByteArrayOutputStream()
        assertEquals(bytes.size.toULong(), PrivateFileStager.copy(stream, output, bytes.size.toULong()))
        assertArrayEquals(bytes, output.toByteArray())
        val limited = ByteArrayOutputStream()
        try {
            PrivateFileStager.copy(ByteArrayInputStream(bytes), limited, 64uL * 1024uL)
            fail("The ceiling must reject the next byte")
        } catch (_: IllegalArgumentException) {
            assertEquals(64 * 1024, limited.size())
        }
    }

    @Test fun completeDescriptorRoundTripsKeysAndUnsignedLength() {
        val remote = RemoteAttachment("https://files.example/" + "a".repeat(64), "a".repeat(64), ByteArray(32) { it.toByte() }, ByteArray(32) { (it + 32).toByte() }, ByteArray(12) { (it + 64).toByte() }, "https", UInt.MAX_VALUE, "../../label.txt")
        assertEquals(remote, AttachmentDescriptor.decode(AttachmentDescriptor.encode(remote)))
        assertEquals(remote.copy(filename = null, contentLength = null), AttachmentDescriptor.decode(AttachmentDescriptor.encode(remote.copy(filename = null, contentLength = null))))
        val invalid = AttachmentDescriptor.encode(remote) + byteArrayOf(1)
        try { AttachmentDescriptor.decode(invalid); fail("Trailing bytes must fail") } catch (_: IllegalArgumentException) { }
    }

    @Test fun acceptedIdWinsOverInterruptedQueuePhase() {
        assertTrue(draftNeedsReview(SendDraftRef("one", "chat", phase = SendPhase.QUEUEING)))
        assertFalse(draftNeedsReview(SendDraftRef("one", "chat", acceptedMessageId = "stored-id", phase = SendPhase.QUEUEING)))
        assertFalse(draftNeedsReview(SendDraftRef("one", "chat", phase = SendPhase.UPLOADING)))
    }
}

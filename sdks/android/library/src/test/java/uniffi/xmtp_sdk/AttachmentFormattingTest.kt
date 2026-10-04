package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AttachmentFormattingTest {
    private val url = "https://files.example.test/object?signature=url-bearer-sentinel"

    private fun checkUrlText(vararg values: Any) {
        for (value in values) {
            val text = value.toString()
            assertFalse(text.contains(url))
            assertFalse(text.contains("url-bearer-sentinel"))
            assertTrue(text.contains("url=<redacted>"))
        }
    }

    @Test
    fun remoteAttachmentTextPreservesStructuredUrl() {
        val value =
            RemoteAttachment(url, "digest", byteArrayOf(1, 2), byteArrayOf(3), byteArrayOf(4), "https", 2u, "file.txt")
        checkUrlText(value, MessageBody.RemoteAttachment(value))
        assertEquals(url, value.url)
        assertArrayEquals(byteArrayOf(1, 2), value.secret)
    }

    @Test
    fun remoteAttachmentTextPreservesStructuredSecret() {
        val secret = byteArrayOf(65, 66, 67, 68)
        val value = RemoteAttachment(url, "digest", secret, byteArrayOf(3), byteArrayOf(4), "https", 2u, "file.txt")
        for (text in listOf(value.toString(), MessageBody.RemoteAttachment(value).toString())) {
            assertFalse(text.contains(secret.contentToString()))
            assertTrue(text.contains("secret=<redacted>"))
        }
        assertArrayEquals(secret, value.secret)
    }

    @Test
    fun attachmentRefTextPreservesStructuredUrl() {
        val value = AttachmentRef("key", url, "digest")
        checkUrlText(value, ClientEvent.AttachmentUploadStarted(value))
        assertEquals(url, value.url)
    }

    @Test
    fun attachmentFailureTextPreservesStructuredUrl() {
        val value = AttachmentFailed("key", url, "digest", "network")
        checkUrlText(value, ClientEvent.AttachmentUploadFailed(value))
        assertEquals(url, value.url)
    }
}

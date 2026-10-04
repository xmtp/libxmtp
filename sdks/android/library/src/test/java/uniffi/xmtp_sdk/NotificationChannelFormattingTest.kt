package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NotificationChannelFormattingTest {
    private fun checkText(
        channel: NotificationChannel,
        secret: String,
        field: String,
    ) {
        for (text in listOf(channel.toString(), NotificationConfig(channel).toString())) {
            assertFalse(text.contains(secret))
            assertTrue(text.contains("$field=<redacted>"))
        }
    }

    @Test
    fun apnsTextKeepsTokenInStructuredField() {
        val channel = NotificationChannel.Apns("apns-reusable-token-sentinel")
        checkText(channel, channel.token, "token")
        assertTrue(channel.token == "apns-reusable-token-sentinel")
    }

    @Test
    fun fcmTextKeepsTokenInStructuredField() {
        val channel = NotificationChannel.Fcm("fcm-reusable-token-sentinel")
        checkText(channel, channel.token, "token")
        assertTrue(channel.token == "fcm-reusable-token-sentinel")
    }

    @Test
    fun httpTextKeepsSigningKeyInStructuredField() {
        val key = ByteArray(32) { (it + 65).toByte() }
        val channel = NotificationChannel.Http("https://example.test/push", key)
        checkText(channel, key.contentToString(), "signingKey")
        assertTrue(channel.url == "https://example.test/push")
        assertArrayEquals(key, channel.signingKey)
    }

    @Test
    fun httpTextKeepsSignedUrlInStructuredField() {
        val url = "https://example.test/push?signature=http-url-bearer-sentinel"
        val channel = NotificationChannel.Http(url, ByteArray(32) { 65 })
        for (text in listOf(channel.toString(), NotificationConfig(channel).toString())) {
            assertFalse(text.contains(url))
            assertFalse(text.contains("http-url-bearer-sentinel"))
            assertTrue(text.contains("url=<redacted>"))
        }
        assertTrue(channel.url == url)
    }
}

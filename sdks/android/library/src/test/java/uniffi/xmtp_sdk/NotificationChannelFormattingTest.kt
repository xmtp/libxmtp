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
        assertTrue(channel.toString().contains(channel.url))
        assertArrayEquals(key, channel.signingKey)
    }
}

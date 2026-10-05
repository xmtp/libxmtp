package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test

class PlatformValuesTest {
    @Test
    fun hexHelpersKeepLowercaseBytesAndEmptyInput() {
        val bytes = byteArrayOf(0, 1, 15, 16, 127, 128.toByte(), 255.toByte())
        assertEquals("00010f107f80ff", bytes.toHex())
        assertArrayEquals(bytes, "00010f107f80ff".hexToByteArray())
        val allBytes = ByteArray(256) { it.toByte() }
        assertArrayEquals(allBytes, allBytes.toHex().hexToByteArray())
        assertEquals("", byteArrayOf().toHex())
        assertArrayEquals(byteArrayOf(), "".hexToByteArray())
    }

    @Test
    fun hexHelpersKeepOddDigitsPrefixAndCharacterDigitRules() {
        assertArrayEquals(byteArrayOf(10, 188.toByte()), "0xaBc".hexToByteArray())
        assertArrayEquals(byteArrayOf(15), "f".hexToByteArray())
        assertArrayEquals(byteArrayOf(), "0x".hexToByteArray())
        // The retained converter removes only the lowercase prefix.
        assertArrayEquals(byteArrayOf(255.toByte(), 15), "0X0f".hexToByteArray())
        assertArrayEquals(byteArrayOf(239.toByte()), "zz".hexToByteArray())
    }

    @Test
    fun inboxHelpersKeepPrefixOnlyValidationAndTypedFailure() {
        validateInboxIds(listOf("", "abc", "z"))
        for (value in listOf("0x", "0X", "0xabc", "0Xabc")) {
            val failure = assertThrows(XmtpException.InvalidArgument::class.java) { validateInboxId(value) }
            assertEquals("InvalidArgument", failure.v1.code)
            assertEquals(ErrorCategory.INPUT, failure.v1.category)
            assertFalse(failure.v1.retryable)
            assertEquals("Invalid inboxId: $value. Inbox IDs cannot start with '0x'.", failure.v1.message)
        }
        val failure =
            assertThrows(XmtpException.InvalidArgument::class.java) { validateInboxIds(listOf("abc", "0Xabc")) }
        assertEquals("Invalid inboxId: 0Xabc. Inbox IDs cannot start with '0x'.", failure.v1.message)
    }
}

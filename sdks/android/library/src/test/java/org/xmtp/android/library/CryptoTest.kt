package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class CryptoTest {
    @Test fun testCodec() =
        runBlocking {
            val message = byteArrayOf(5, 5, 5)
            val encrypted = encryptBytes(message)
            assertFalse(message.contentEquals(encrypted.ciphertext))
            assertArrayEquals(message, decryptBytes(encrypted.ciphertext, encrypted.keys))
            val changed = encrypted.ciphertext.copyOf()
            changed[0] = (changed[0].toInt() xor 1).toByte()
            assertTrue(runCatching { decryptBytes(changed, encrypted.keys) }.exceptionOrNull() is XmtpException)
        }

    @Test fun testDecryptingKnownCypherText() =
        runBlocking {
            // Fixed fields from the original XMTP JavaScript ciphertext fixture.
            val salt =
                listOf(
                    23,
                    10,
                    217,
                    190,
                    235,
                    216,
                    145,
                    38,
                    49,
                    224,
                    165,
                    169,
                    22,
                    55,
                    152,
                    150,
                    176,
                    65,
                    207,
                    91,
                    45,
                    45,
                    16,
                    171,
                    146,
                    125,
                    143,
                    60,
                    152,
                    128,
                    0,
                    120,
                ).map { it.toByte() }.toByteArray()
            val nonce =
                listOf(
                    219,
                    247,
                    207,
                    184,
                    141,
                    179,
                    171,
                    100,
                    251,
                    171,
                    120,
                    137,
                ).map { it.toByte() }.toByteArray()
            val ciphertext =
                listOf(216, 215, 152, 167, 118, 59, 93, 177, 53, 242, 147, 10, 87, 143, 27, 245, 154, 169, 109)
                    .map { it.toByte() }
                    .toByteArray()
            val keys =
                EncryptionKeys(
                    byteArrayOf(1, 2, 3, 4),
                    salt,
                    nonce,
                    "68bd4ef27c192622cb66b1da180ddd78e7f928a9b31c319257ab297bbf186da4",
                    19uL,
                )
            assertArrayEquals(byteArrayOf(5, 5, 5), decryptBytes(ciphertext, keys))
        }
}

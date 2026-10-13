package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalSigningKeyTest {
    // verifies: IDENT-088, IDENT-089
    @Test
    fun exportRestoreAndSignerConformance() =
        runBlocking {
            val key = SDKClient.generateKeys()
            val (bytes, hex) = exportKey(key)
            assertEquals(32, bytes.size)
            val restored = LocalSigningKey.fromHex(hex)
            assertArrayEquals(bytes, restored.toBytes())
            bytes.fill(0)
            assertEquals(hex, key.toHex())
            val owner: Signer = LocalSigningKey.fromBytes(restored.toBytes())
            assertEquals(key.identity(), owner.identity())
            withClients {
                val client = create(restored)
                assertTrue(client.isRegistered())
            }
        }

    @Test
    fun knownKeyAndInvalidImports() =
        runBlocking {
            val fixed = "0x" + "0".repeat(63) + "1"
            val key = LocalSigningKey.fromHex(fixed)
            assertEquals("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf", key.identity().identifier)
            assertEquals(fixed, key.toHex())
            assertFalse(key.toString().contains(fixed.drop(2)))
            for (bytes in listOf(ByteArray(31), ByteArray(32), ByteArray(32) { -1 })) {
                val error = runCatching { LocalSigningKey.fromBytes(bytes) }.exceptionOrNull()
                assertTrue(error is XmtpException.InvalidInput)
            }
        }
}

// This ordinary function is a compile-time check that mobile exports do not suspend.
private fun exportKey(key: LocalSigningKey): Pair<ByteArray, String> = key.toBytes() to key.toHex()

package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalSigningKeyTest {
    // Export, restore and the signer identity rules are Rust's:
    // xmtp_sdk/src/signer/local_tests.rs. This checks only that the generated
    // LocalSigningKey is a Signer that client creation accepts directly.
    @Test
    fun localSigningKeyIsAcceptedDirectlyAsASigner() =
        runBlocking {
            val key = SDKClient.generateKeys()
            withClients {
                val client = create(key)
                assertTrue(client.isRegistered())
                assertEquals(key.identity(), client.identity())
            }
        }

    @Test
    fun knownKeyAndInvalidImports() =
        runBlocking {
            val fixed = "0x" + "0".repeat(63) + "1"
            val key = LocalSigningKey.fromHex(fixed)
            assertEquals("0x7e5f4552091a69125d5dfcb7b8c2659029395bdf", key.identity().identifier)
            val (bytes, hex) = exportKey(key)
            assertEquals(32, bytes.size)
            assertEquals(fixed, hex)
            assertFalse(key.toString().contains(fixed.drop(2)))
            for (invalid in listOf(ByteArray(31), ByteArray(32), ByteArray(32) { -1 })) {
                val error = runCatching { LocalSigningKey.fromBytes(invalid) }.exceptionOrNull()
                assertTrue(error is XmtpException.InvalidInput)
            }
        }
}

// This ordinary function is a compile-time check that mobile exports do not suspend.
private fun exportKey(key: LocalSigningKey): Pair<ByteArray, String> = key.toBytes() to key.toHex()

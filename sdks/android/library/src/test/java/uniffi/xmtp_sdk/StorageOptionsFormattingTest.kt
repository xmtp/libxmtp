package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StorageOptionsFormattingTest {
    private val key = ByteArray(32) { (it + 65).toByte() }

    private fun storage() = StorageOptions(StorageLocation.InMemory, label = "diagnostic-test", encryptionKey = key)

    @Test
    fun directStorageTextKeepsEncryptionKeyInStructuredField() {
        val options = storage()
        assertFalse(options.toString().contains(key.contentToString()))
        assertTrue(options.toString().contains("encryptionKey=<redacted>"))
        assertTrue(options.toString().contains("label=diagnostic-test"))
        assertArrayEquals(key, options.encryptionKey)
    }

    @Test
    fun nestedClientTextKeepsEncryptionKeyInStructuredField() {
        val options = ClientOptions(storage = storage())
        assertFalse(options.toString().contains(key.contentToString()))
        assertTrue(options.toString().contains("encryptionKey=<redacted>"))
        assertArrayEquals(key, options.storage.encryptionKey)
    }
}

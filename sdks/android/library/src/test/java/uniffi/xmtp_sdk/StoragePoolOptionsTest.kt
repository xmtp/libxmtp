package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Rust owns the pool rules: xmtp_sdk/src/tests/storage.rs::storage_pool_options_round_trip_and_reject_an_inverted_range.
// This lowers each pool form into a native client and lifts it back from
// options(): an absent pool, absent fields, both fields and one field. The
// forms are the ones the deleted Kotlin conformance check (RetainedOptions.kt)
// used. AndroidContextStartupTest checks only the form with both fields.
class StoragePoolOptionsTest {
    private val options = liveOptions().copy(registration = RegistrationOptions(auto = false))

    private fun withPool(pool: StoragePoolOptions?) = options.copy(storage = options.storage.copy(pool = pool))

    @Test
    fun eachPoolFormCrossesTheNativeBoundary() =
        runBlocking {
            withTimeout(60_000) {
                val pools =
                    listOf(
                        null,
                        StoragePoolOptions(),
                        StoragePoolOptions(min = 2u, max = 10u),
                        StoragePoolOptions(max = 7u),
                    )
                withClients {
                    for (pool in pools) {
                        assertEquals("pool $pool", pool, create(options = withPool(pool)).options().storage.pool)
                    }
                    val inverted = runCatching { create(options = withPool(StoragePoolOptions(min = 4u, max = 2u))) }
                    val failure = inverted.exceptionOrNull()
                    assertTrue("Expected InvalidInput, got $failure", failure is XmtpException.InvalidInput)
                    val details = (failure as XmtpException.InvalidInput).v1
                    assertEquals(ErrorCategory.INPUT, details.category)
                    assertFalse(details.retryable)
                }
            }
        }
}

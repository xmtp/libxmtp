package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.xmtp_sdk.*

class VisibilityConfirmationOptionsTest {
    private class RecordingClient : Client(NoHandle) {
        val calls = mutableListOf<ULong?>()

        override fun clientKey(): ULong = 900uL

        override suspend fun catchUpToLive(timeoutMs: ULong?): CatchUpSummary {
            calls.add(timeoutMs)
            return CatchUpSummary(0uL, 0uL, 0uL, true)
        }
    }

    // The approved facade accepts the optional timeout directly.
    @Test fun toFfi_mapsAllFields() =
        runBlocking {
            val raw = RecordingClient()
            val client = testSDKClient(raw)
            client.catchUpToLive(10_000uL)
            client.catchUpToLive(ULong.MAX_VALUE)
            assertEquals(listOf(10_000uL, ULong.MAX_VALUE), raw.calls)
        }

    @Test fun toFfi_defaultsToAllNull() =
        runBlocking {
            val raw = RecordingClient()
            testSDKClient(raw).catchUpToLive(null)
            assertEquals(1, raw.calls.size)
            assertNull(raw.calls.single())
        }
}

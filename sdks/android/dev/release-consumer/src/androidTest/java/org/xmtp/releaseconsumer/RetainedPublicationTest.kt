package org.xmtp.releaseconsumer

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.Timestamp
import uniffi.xmtp_sdk.sdkVersion
import java.util.Date

@RunWith(AndroidJUnit4::class)
class RetainedPublicationTest {
    @Test fun retainedNativeVersionAndMinimumApi() {
        assertEquals(BuildConfig.EXPECTED_VERSION, sdkVersion())
        val instant = Timestamp(1_000_000_001L).date
        assertEquals(1L, instant.epochSecond)
        assertEquals(1, instant.nano)
        assertEquals(1_000L, Date.from(instant).time)
    }
}

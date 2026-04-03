package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class VisibilityConfirmationOptionsTest {
    @Test
    fun toFfi_mapsAllFields() {
        val options =
            VisibilityConfirmationOptions(
                quorumPercentage = 0.75f,
                quorumAbsolute = 3u,
                timeoutMs = 10_000u,
            )
        val ffi = options.toFfi()
        assertEquals(0.75f, ffi.quorumPercentage)
        assertEquals(3.toULong(), ffi.quorumAbsolute)
        assertEquals(10_000.toULong(), ffi.timeoutMs)
    }

    @Test
    fun toFfi_defaultsToAllNull() {
        val options = VisibilityConfirmationOptions()
        val ffi = options.toFfi()
        assertNull(ffi.quorumPercentage)
        assertNull(ffi.quorumAbsolute)
        assertNull(ffi.timeoutMs)
    }
}

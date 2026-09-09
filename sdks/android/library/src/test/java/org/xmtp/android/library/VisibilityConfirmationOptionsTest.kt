package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class VisibilityConfirmationOptionsTest {
    @Test
    fun toFfi_mapsAllFields() {
        val options =
            VisibilityConfirmationOptions(
                timeoutMs = 10_000u,
            )
        val ffi = options.toFfi()
        assertEquals(10_000.toULong(), ffi.timeoutMs)
    }

    @Test
    fun toFfi_defaultsToAllNull() {
        val options = VisibilityConfirmationOptions()
        val ffi = options.toFfi()
        assertNull(ffi.timeoutMs)
    }
}

package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.xmtp_sdk.StoragePoolOptions

class DbPoolOptionsTest {
    @Test
    fun defaultsToAllNull() {
        val options = StoragePoolOptions()
        assertNull(options.max)
        assertNull(options.min)
    }

    @Test
    fun carriesValuesThrough() {
        val options = StoragePoolOptions(max = 10u, min = 2u)
        assertEquals(10u, options.max)
        assertEquals(2u, options.min)
    }

    @Test
    fun acceptsPartialFields() {
        val options = StoragePoolOptions(max = 7u)
        assertEquals(7u, options.max)
        assertNull(options.min)
    }
}

package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.xmtp_sdk.BackendOptions

class ClientCacheKeyTest {
    @Test
    fun testApiClientCacheKeysDifferentConfigurations() {
        val api = BackendOptions(url = "http://10.0.2.2:5050")
        val second = api.copy(url = "https://backend.example.com")
        val third = api.copy(url = "https://other.example.com")
        assertNotEquals(api, second)
        assertNotEquals(second, third)
        assertNotEquals(api, third)

        val versionOne = api.copy(appVersion = "1.0.0")
        val versionTwo = api.copy(appVersion = "2.0.0")
        assertNotEquals(versionOne, versionTwo)
        assertNotEquals(api, versionOne)
        assertNotEquals(api, api.copy(appVersion = ""))
        assertNotEquals(versionOne, second.copy(appVersion = "2.0.0"))

        assertEquals(versionOne, versionOne.copy())
        assertEquals(api, BackendOptions(url = "http://10.0.2.2:5050"))
        // An absent version must not collide with the literal string "null".
        assertNotEquals(api, api.copy(appVersion = "null"))
        assertNull(api.appVersion)
        assertEquals("1.0.0", versionOne.appVersion)
    }

    @Test
    fun testPreservesBackendUrlUntilNativeValidation() {
        for (url in listOf("", " ", "\t\n")) {
            assertEquals(url, BackendOptions(url = url).url)
        }
    }
}

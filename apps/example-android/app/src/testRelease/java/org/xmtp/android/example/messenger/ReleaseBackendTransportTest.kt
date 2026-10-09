package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig

class ReleaseBackendTransportTest {
    @Test fun buildModeDefinesTheEmulatorHttpBoundary() {
        assertFalse(BuildConfig.DEBUG)
        for (url in listOf("http://10.0.2.2:5050", "http://example.com")) {
            assertTrue(url, runCatching { validatedBackendUrl(url) }.exceptionOrNull() is IllegalArgumentException)
            assertFalse(url, localAttachmentNetwork(url))
        }
        for (url in listOf("http://localhost:5050", "http://127.0.0.1:5050", "http://[::1]:5050")) {
            assertEquals(url, validatedBackendUrl(url))
            assertTrue(url, localAttachmentNetwork(url))
        }
        assertEquals("https://10.0.2.2:5050", validatedBackendUrl("https://10.0.2.2:5050"))
    }
}

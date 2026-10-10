package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.BuildConfig

class DebugBackendTransportTest {
    @Test fun buildModeDefinesTheEmulatorHttpBoundary() {
        assertTrue(BuildConfig.DEBUG)
        assertEquals("http://10.0.2.2:5050", validatedBackendUrl("http://10.0.2.2:5050"))
        assertTrue(localAttachmentNetwork("http://10.0.2.2:5050"))
    }
}

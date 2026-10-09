package org.xmtp.android.example.messenger

import org.junit.Assert.*
import org.junit.Test

class LocalAttachmentNetworkTest {
    @Test fun localTransportIsRestrictedToSupportedLoopbackRoutes() {
        for (url in listOf(
            "http://localhost:5250",
            "http://127.0.0.1:5250",
            "http://10.0.2.2:5250",
            "http://[::1]:5250",
        )) {
            assertTrue(url, localAttachmentNetwork(url))
        }
        for (url in listOf(
            "https://example.com",
            "http://192.168.1.1:5250",
            "https://localhost.example",
            "ftp://127.0.0.1",
            "invalid",
        )) {
            assertFalse(url, localAttachmentNetwork(url))
        }
    }
}

package org.xmtp.android.library

import org.junit.Test

class ApiClientTest {
    @Test fun testCanGetApiClient() {
        ApiClient(environment = XMTPEnvironment.LOCAL)
    }
}

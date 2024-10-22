package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SmartContractWalletTest {
    @Test
    fun testCanCreateASCW() {
        val key = byteArrayOf(
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
            0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
            0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F
        )
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val davonSCW = FakeSCWWallet.generate()
        val options = ClientOptions(
            ClientOptions.Api(XMTPEnvironment.LOCAL, false),
            enableV3 = true,
            appContext = context,
            dbEncryptionKey = key
        )
        val davonSCWClient = runBlocking {
            Client().createV3(
                account = davonSCW,
                options = options
            )
        }
        val davonSCWClient2 = runBlocking {
            Client().buildV3(
                address = davonSCW.address,
                chainId = davonSCW.chainId,
                options = options
            )
        }

        assertEquals(davonSCWClient.inboxId, davonSCWClient2.inboxId)
    }
}

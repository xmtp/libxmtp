package com.example.xmtpv3_example

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking

import org.junit.Test
import org.junit.runner.RunWith

import org.junit.Assert.*
import org.junit.FixMethodOrder
import org.junit.runners.MethodSorters
import org.web3j.crypto.Credentials
import org.web3j.crypto.ECKeyPair
import java.security.SecureRandom

/**
 * Instrumented test, which will execute on an Android device.
 *
 * See [testing documentation](http://d.android.com/tools/testing).
 */
@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class ExampleInstrumentedTest {
    companion object {
        val privateKey: ByteArray = SecureRandom().generateSeed(32)
        val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
        val inboxOwner = Web3jInboxOwner(credentials)
        val logger = AndroidFfiLogger()
    }

    @Test
    fun testAHappyPath() {
        runBlocking {
            val client = uniffi.xmtpv3.createClient(logger, inboxOwner, EMULATOR_LOCALHOST_ADDRESS, false)
            assertNotNull("Should be able to construct client", client.walletAddress())
            client.close()
        }
    }

    @Test
    fun testBHappyPath() {
        runBlocking {
            val client = uniffi.xmtpv3.createClient(logger, inboxOwner, EMULATOR_LOCALHOST_ADDRESS, false)
            assertNotNull("Should be able to construct client", client.walletAddress())
            client.close()
        }
    }

    @Test
    fun testErrorThrows() {
        runBlocking {
            var didThrow = false;
            try {
                val client = uniffi.xmtpv3.createClient(logger, inboxOwner, "http://incorrect:5556", false)
            } catch (e: Exception) {
                didThrow = true
            }
            assertEquals("Should throw exception", true, didThrow)
        }
    }

    @Test
    fun testFHappyPath() {
        runBlocking {
            val client = uniffi.xmtpv3.createClient(logger, inboxOwner, EMULATOR_LOCALHOST_ADDRESS, false)
            assertNotNull("Should be able to construct client", client.walletAddress())
            client.close()
        }
    }
}
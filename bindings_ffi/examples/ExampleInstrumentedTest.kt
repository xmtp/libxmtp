package com.example.xmtpv3_example

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking

import org.junit.Test
import org.junit.runner.RunWith

import org.junit.Assert.*
import org.web3j.crypto.Credentials
import org.web3j.crypto.ECKeyPair
import java.security.SecureRandom

/**
 * Instrumented test, which will execute on an Android device.
 *
 * See [testing documentation](http://d.android.com/tools/testing).
 */
@RunWith(AndroidJUnit4::class)
class ExampleInstrumentedTest {
    @Test
    fun testHappyPath() {
        val privateKey: ByteArray = SecureRandom().generateSeed(32)
        val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
        val inboxOwner = Web3jInboxOwner(credentials)
        runBlocking {
            val client = uniffi.xmtpv3.createClient(AndroidFfiLogger(), inboxOwner, EMULATOR_LOCALHOST_ADDRESS, false)
            assertNotNull("Should be able to construct client", client.walletAddress())
            client.close()
        }
    }

    @Test
    fun testErrorThrows() {
        val privateKey: ByteArray = SecureRandom().generateSeed(32)
        val credentials: Credentials = Credentials.create(ECKeyPair.create(privateKey))
        val inboxOwner = Web3jInboxOwner(credentials)
        runBlocking {
            var didThrow = false;
            try {
                val client = uniffi.xmtpv3.createClient(AndroidFfiLogger(), inboxOwner, "http://incorrect:5556", false)
            } catch (e: Exception) {
                didThrow = true
            }
            assertEquals("Should throw exception", true, didThrow)
        }
    }
}
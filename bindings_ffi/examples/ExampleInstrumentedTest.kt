package com.example.xmtpv3_example

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking

import org.junit.Test
import org.junit.runner.RunWith

import org.junit.Assert.*

/**
 * Instrumented test, which will execute on an Android device.
 *
 * See [testing documentation](http://d.android.com/tools/testing).
 */
@RunWith(AndroidJUnit4::class)
class ExampleInstrumentedTest {
    @Test
    fun testHappyPath() {
        runBlocking {
            val client = uniffi.xmtpv3.createClient(EMULATOR_LOCALHOST_ADDRESS, false);
            assertNotNull("Should be able to construct client", client.walletAddress())
        }
    }

    @Test
    fun testErrorThrows() {
        runBlocking {
            var didThrow = false;
            try {
                val client = uniffi.xmtpv3.createClient("http://incorrect:5556", false);
            } catch (e: Exception) {
                didThrow = true;
            }
            assertEquals("Should throw exception", true, didThrow)
        }
    }
}
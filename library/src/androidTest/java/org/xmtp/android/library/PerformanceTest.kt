package org.xmtp.android.library

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert
import org.junit.BeforeClass
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import java.security.SecureRandom
import java.util.Date
import kotlin.system.measureTimeMillis

@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class PerformanceTest {
    companion object {
        private lateinit var alixWallet: PrivateKeyBuilder
        private lateinit var boWallet: PrivateKeyBuilder
        private lateinit var caroWallet: PrivateKeyBuilder
        private lateinit var davonWallet: PrivateKeyBuilder
        private lateinit var eriWallet: PrivateKeyBuilder
        private lateinit var alix: PrivateKey
        private lateinit var alixClient: Client
        private lateinit var bo: PrivateKey
        private lateinit var boClient: Client
        private lateinit var caro: PrivateKey
        private lateinit var caroClient: Client
        private lateinit var davon: PrivateKey
        private lateinit var davonClient: Client
        private lateinit var eri: PrivateKey
        private lateinit var eriClient: Client
        private var dm: Dm? = null
        private var group: Group? = null

        @BeforeClass
        @JvmStatic
        fun setUpClass() {
            val fixtures = fixtures(ClientOptions.Api(XMTPEnvironment.DEV, true))
            alixWallet = fixtures.alixAccount
            alix = fixtures.alix
            boWallet = fixtures.boAccount
            bo = fixtures.bo
            caroWallet = fixtures.caroAccount
            caro = fixtures.caro
            davonWallet = fixtures.davonAccount
            davon = fixtures.davon
            eriWallet = fixtures.eriAccount
            eri = fixtures.eri

            alixClient = fixtures.alixClient
            boClient = fixtures.boClient
            caroClient = fixtures.caroClient
            davonClient = fixtures.davonClient
            eriClient = fixtures.eriClient
        }
    }

    @Test
    fun test1_CreateDM() = runBlocking {
        val time = measureTimeMillis {
            dm = alixClient.conversations.findOrCreateDm(boClient.address)
        }
        Log.d("PERF", "created a DM in: ${time}ms")
        assert(time < 200)
    }

    @Test
    fun test2_SendGm() = runBlocking {
        val gmMessage = "gm-" + (1..999999).random().toString()
        val time = measureTimeMillis {
            dm!!.send(gmMessage)
        }
        Log.d("PERF", "sendGmTime: ${time}ms")
        assert(time < 200)
    }

    @Test
    fun test3_CreateGroup() = runBlocking {
        val time = measureTimeMillis {
            group = alixClient.conversations.newGroup(
                listOf(
                    boClient.address,
                    caroClient.address,
                    davonClient.address
                )
            )
        }
        Log.d("PERF", "createGroupTime: ${time}ms")
        assert(time < 200)
    }

    @Test
    fun test4_SendGmInGroup() = runBlocking {
        val groupMessage = "gm-" + (1..999999).random().toString()
        val time = measureTimeMillis {
            group!!.send(groupMessage)
        }
        Log.d("PERF", "sendGmInGroupTime: ${time}ms")
        assert(time < 200)
    }

    @Test
    fun testCreatesADevClientPerformance() {
        val key = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val fakeWallet = PrivateKeyBuilder()
        val start = Date()
        val client = runBlocking {
            Client().create(
                account = fakeWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.DEV, true),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val end = Date()
        val time1 = end.time - start.time
        Log.d("PERF", "Created a client in ${time1 / 1000.0}s")

        val start2 = Date()
        val buildClient1 = runBlocking {
            Client().build(
                fakeWallet.address,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.DEV, true),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }
        val end2 = Date()
        val time2 = end2.time - start2.time
        Log.d("PERF", "Built a client in ${time2 / 1000.0}s")

        val start3 = Date()
        val buildClient2 = runBlocking {
            Client().build(
                fakeWallet.address,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.DEV, true),
                    appContext = context,
                    dbEncryptionKey = key
                ),
                inboxId = client.inboxId
            )
        }
        val end3 = Date()
        val time3 = end3.time - start3.time
        Log.d("PERF", "Built a client with inboxId in ${time3 / 1000.0}s")

        runBlocking { Client.connectToApiBackend(ClientOptions.Api(XMTPEnvironment.DEV, true)) }
        val start4 = Date()
        runBlocking {
            Client().create(
                PrivateKeyBuilder(),
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.DEV, true),
                    appContext = context,
                    dbEncryptionKey = key
                ),
            )
        }
        val end4 = Date()
        val time4 = end4.time - start4.time
        Log.d("PERF", "Create a client after prebuilding apiClient in ${time4 / 1000.0}s")

        assert(time2 < time1)
        assert(time3 < time1)
        assert(time3 < time2)
        assert(time4 < time1)
        Assert.assertEquals(client.inboxId, buildClient1.inboxId)
        Assert.assertEquals(client.inboxId, buildClient2.inboxId)
    }
}

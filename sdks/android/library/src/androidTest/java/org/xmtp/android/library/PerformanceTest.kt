package org.xmtp.android.library

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import uniffi.xmtp_sdk.*
import kotlin.system.measureTimeMillis

@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class PerformanceTest : BaseInstrumentedTest() {
    private lateinit var alixClient: SDKClient
    private lateinit var boClient: SDKClient
    private lateinit var caroClient: SDKClient
    private lateinit var davonClient: SDKClient
    private lateinit var eriClient: SDKClient

    @Before
    override fun setUp() {
        super.setUp()
        val fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
        davonClient = runBlocking { createClient(createWallet()) }
        eriClient = runBlocking { createClient(createWallet()) }
    }

    @Test
    fun test1_CreateDM() =
        runBlocking {
            val time =
                measureTimeMillis {
                    alixClient.conversations().createDm(boClient.inboxId())
                }
            Log.d("PERF", "created a DM in: ${time}ms")
            assertTrue("Create must finish in less than 400 ms; actual=$time", time < 400)
        }

    @Test
    fun test2_SendGm() =
        runBlocking {
            val dm = alixClient.conversations().createDm(boClient.inboxId())
            val gmMessage = "gm-" + (1..999999).random().toString()
            val time =
                measureTimeMillis {
                    dm.sendText(gmMessage)
                }
            Log.d("PERF", "sendGmTime: ${time}ms")
            assertTrue("Send must finish in less than 200 ms; actual=$time", time < 200)
        }

    @Test
    fun test3_CreateGroup() =
        runBlocking {
            val time =
                measureTimeMillis {
                    alixClient.conversations().createGroup(
                        listOf(
                            boClient.inboxId(),
                            caroClient.inboxId(),
                            davonClient.inboxId(),
                        ),
                    )
                }
            Log.d("PERF", "createGroupTime: ${time}ms")
            assertTrue("Create must finish in less than 400 ms; actual=$time", time < 400)
        }

    @Test
    fun test4_SendGmInGroup() =
        runBlocking {
            val groupMessage = "gm-" + (1..999999).random().toString()
            val group =
                alixClient.conversations().createGroup(
                    listOf(
                        boClient.inboxId(),
                    ),
                )
            val time =
                measureTimeMillis {
                    group.sendText(groupMessage)
                }
            Log.d("PERF", "sendGmInGroupTime: ${time}ms")
            assertTrue("Send must finish in less than 200 ms; actual=$time", time < 200)
        }
}

package org.xmtp.android.library

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Before
import org.junit.FixMethodOrder
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.MethodSorters
import kotlin.system.measureTimeMillis

@RunWith(AndroidJUnit4::class)
@FixMethodOrder(MethodSorters.NAME_ASCENDING)
class PerformanceTest : BaseInstrumentedTest() {
    private lateinit var alixClient: Client
    private lateinit var boClient: Client
    private lateinit var caroClient: Client
    private lateinit var davonClient: Client
    private lateinit var eriClient: Client

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
                    alixClient.conversations.findOrCreateDm(boClient.inboxId)
                }
            Log.d("PERF", "created a DM in: ${time}ms")
            assert(time < 400)
        }

    @Test
    fun test2_SendGm() =
        runBlocking {
            val dm = alixClient.conversations.findOrCreateDm(boClient.inboxId)
            val gmMessage = "gm-" + (1..999999).random().toString()
            val time =
                measureTimeMillis {
                    dm.send(gmMessage)
                }
            Log.d("PERF", "sendGmTime: ${time}ms")
            assert(time < 200)
        }

    @Test
    fun test3_CreateGroup() =
        runBlocking {
            val time =
                measureTimeMillis {
                    alixClient.conversations.newGroup(
                        listOf(
                            boClient.inboxId,
                            caroClient.inboxId,
                            davonClient.inboxId,
                        ),
                    )
                }
            Log.d("PERF", "createGroupTime: ${time}ms")
            assert(time < 400)
        }

    @Test
    fun test4_SendGmInGroup() =
        runBlocking {
            val groupMessage = "gm-" + (1..999999).random().toString()
            val group =
                alixClient.conversations.newGroup(
                    listOf(
                        boClient.inboxId,
                    ),
                )
            val time =
                measureTimeMillis {
                    group.send(groupMessage)
                }
            Log.d("PERF", "sendGmInGroupTime: ${time}ms")
            assert(time < 200)
        }
}

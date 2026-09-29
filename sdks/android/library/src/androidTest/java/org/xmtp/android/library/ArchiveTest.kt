package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.libxmtp.ArchiveElement
import org.xmtp.android.library.libxmtp.ArchiveOptions
import uniffi.xmtpv3.FfiConversationMessageKind
import java.io.File
import java.security.SecureRandom

@RunWith(AndroidJUnit4::class)
class ArchiveTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
    }

    @Test
    fun testClientArchives() {
        val encryptionKey = SecureRandom().generateSeed(32)
        val alixWallet = createWallet()

        val alixClient = runBlocking { createClient(alixWallet) }

        val directoryFile = File(context.filesDir.absolutePath, "testing_all")
        val consentFile = File(context.filesDir.absolutePath, "testing_consent")

        directoryFile.mkdirs()
        consentFile.mkdirs()

        val allPath = directoryFile.absolutePath + "/testAll.zstd"
        val consentPath = consentFile.absolutePath + "/testConsent.zstd"

        val group = runBlocking { alixClient.conversations.newGroup(listOf(boClient.inboxId)) }
        runBlocking {
            group.send("hi")
            alixClient.conversations.syncAllConversations()
            boClient.conversations.syncAllConversations()
        }
        val boGroup = runBlocking { boClient.conversations.findGroup(group.id)!! }

        runBlocking { alixClient.createArchive(allPath, encryptionKey) }
        runBlocking {
            alixClient.createArchive(
                consentPath,
                encryptionKey,
                opts = ArchiveOptions(archiveElements = listOf(ArchiveElement.CONSENT)),
            )
        }

        val metadataAll = runBlocking { alixClient.archiveMetadata(allPath, encryptionKey) }
        val metadataConsent = runBlocking { alixClient.archiveMetadata(consentPath, encryptionKey) }

        assertEquals(metadataAll.elements.size, 2)
        assertEquals(metadataConsent.elements, listOf(ArchiveElement.CONSENT))

        val alixClient2 = runBlocking { createClient(alixWallet) }

        runBlocking {
            alixClient2.importArchive(allPath, encryptionKey)
            alixClient.conversations.syncAllConversations()
            delay(2000)
            alixClient2.conversations.syncAllConversations()
            delay(2000)
            alixClient.preferences.sync()
            delay(2000)
            alixClient2.preferences.sync()
            delay(2000)
            boGroup.send("hey")
            boClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
        }
        val convosList = runBlocking { alixClient2.conversations.list() }
        assertEquals(1, convosList.size)
        runBlocking {
            convosList.first().sync()
            assertEquals(runBlocking { convosList.first().messages() }.size, 3)
            assertEquals(convosList.first().consentState(), ConsentState.ALLOWED)
        }
    }

    @Test
    fun testInActiveDmsStitchIfDuplicated() {
        Client.register(codec = GroupUpdatedCodec())
        val encryptionKey = SecureRandom().generateSeed(32)
        val alixWallet = createWallet()
        val alixClient = runBlocking { createClient(alixWallet) }
        val directoryFile = File(context.filesDir.absolutePath, "testing_all")
        directoryFile.mkdirs()
        val allPath = directoryFile.absolutePath + "/testAll.zstd"

        val dm = runBlocking { alixClient.conversations.findOrCreateDm(boClient.inboxId) }
        val archivedId = runBlocking { dm.send("archived from alix") }
        runBlocking {
            alixClient.conversations.syncAllConversations()
            boClient.conversations.syncAllConversations()
        }
        val boDm = runBlocking { boClient.conversations.findDmByInboxId(alixClient.inboxId)!! }
        runBlocking { alixClient.createArchive(allPath, encryptionKey) }
        val alixClient2 = runBlocking { createClient(alixWallet) }
        runBlocking { alixClient2.importArchive(allPath, encryptionKey) }
        val restored =
            runBlocking {
                requireNotNull(alixClient2.conversations.findConversation(dm.id) as? Conversation.Dm).dm
            }
        assertFalse(runBlocking { restored.isActive() })
        assertEquals(1, runBlocking { alixClient2.conversations.listDms().size })

        val dm2 = runBlocking { alixClient2.conversations.findOrCreateDm(boClient.inboxId) }
        assertNotEquals("The restored client must create a second physical DM", dm.id, dm2.id)
        assertTrue(runBlocking { dm2.isActive() })
        assertEquals(1, runBlocking { alixClient2.conversations.listDms().size })

        val alix2Id = runBlocking { dm2.send("live from alix installation 2") }
        val boDm2 =
            runBlocking {
                boClient.conversations.syncAllConversations()
                requireNotNull(boClient.conversations.findConversation(dm2.id) as? Conversation.Dm).dm
            }
        assertEquals(dm2.id, boDm2.id)
        val boId = runBlocking { boDm2.send("reply from bo in the duplicate") }

        runBlocking {
            alixClient2.conversations.syncAllConversations()
            dm2.sync()
            boDm2.sync()
            boDm.sync()
        }
        assertEquals(1, runBlocking { alixClient2.conversations.listDms().size })

        data class ExpectedApplication(
            val body: String,
            val sender: String,
            val physicalGroup: String,
        )

        val expectedApplications =
            mapOf(
                archivedId to ExpectedApplication("archived from alix", alixClient.inboxId, dm.id),
                alix2Id to ExpectedApplication("live from alix installation 2", alixClient2.inboxId, dm2.id),
                boId to ExpectedApplication("reply from bo in the duplicate", boClient.inboxId, dm2.id),
            )

        suspend fun assertStitchedHistory(
            label: String,
            conversation: Dm,
        ) {
            val messages = conversation.messages()
            val rawCount = conversation.countMessages()
            val rows =
                messages.joinToString { message ->
                    "${message.id}:${message.kind}:${message.conversationId}:${message.senderInboxId}:${message.body}"
                }
            assertEquals(
                "$label has duplicate IDs; raw count=$rawCount; rows=$rows",
                messages.size,
                messages
                    .map {
                        it.id
                    }.toSet()
                    .size,
            )
            val applications = messages.filter { it.kind == FfiConversationMessageKind.APPLICATION }
            assertEquals(
                "$label application IDs; raw count=$rawCount; rows=$rows",
                expectedApplications.keys,
                applications.map { it.id }.toSet(),
            )
            assertEquals("$label application count; rows=$rows", expectedApplications.size, applications.size)
            for (message in applications) {
                val expected = requireNotNull(expectedApplications[message.id])
                assertEquals("$label body for ${message.id}", expected.body, message.body)
                assertEquals("$label sender for ${message.id}", expected.sender, message.senderInboxId)
                assertEquals("$label physical group for ${message.id}", expected.physicalGroup, message.conversationId)
            }
            assertEquals(
                "$label must include both physical DMs; rows=$rows",
                setOf(dm.id, dm2.id),
                applications.map { it.conversationId }.toSet(),
            )
        }

        runBlocking {
            assertStitchedHistory("restored Alix DM", restored)
            assertStitchedHistory("joined Alix DM", dm2)
            assertStitchedHistory("original Bo DM", boDm)
            assertStitchedHistory("joined Bo DM", boDm2)
        }
    }

    @Test
    fun testImportArchiveWorksEvenOnFullDatabase() {
        val encryptionKey = SecureRandom().generateSeed(32)
        val directoryFile = File(context.filesDir.absolutePath, "testing_all")

        directoryFile.mkdirs()

        val allPath = directoryFile.absolutePath + "/testAll.zstd"

        val group = runBlocking { alixClient.conversations.newGroup(listOf(boClient.inboxId)) }
        val dm = runBlocking { alixClient.conversations.findOrCreateDm(boClient.inboxId) }
        runBlocking {
            group.send("First")
            dm.send("hi")
            alixClient.conversations.syncAllConversations()
            boClient.conversations.syncAllConversations()
        }
        val boGroup = runBlocking { boClient.conversations.findGroup(group.id)!! }

        assertEquals(runBlocking { group.messages() }.size, 2)
        assertEquals(runBlocking { boGroup.messages() }.size, 2)
        assertEquals(runBlocking { alixClient.conversations.list() }.size, 2)
        assertEquals(runBlocking { boClient.conversations.list() }.size, 2)

        runBlocking { alixClient.createArchive(allPath, encryptionKey) }
        runBlocking { group.send("Second") }
        runBlocking { alixClient.importArchive(allPath, encryptionKey) }
        runBlocking {
            group.send("Third")
            dm.send("hi")
            alixClient.conversations.syncAllConversations()
            boClient.conversations.syncAllConversations()
        }
        assertEquals(runBlocking { group.messages() }.size, 4)
        assertEquals(runBlocking { boGroup.messages() }.size, 4)
        assertEquals(runBlocking { alixClient.conversations.list() }.size, 2)
        assertEquals(runBlocking { boClient.conversations.list() }.size, 2)
    }
}

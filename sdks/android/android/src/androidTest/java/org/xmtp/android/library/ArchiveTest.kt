package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.libxmtp.ArchiveElement
import org.xmtp.android.library.libxmtp.ArchiveOptions
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
        val encryptionKey = SecureRandom().generateSeed(32)
        val alixWallet = createWallet()

        val alixClient = runBlocking { createClient(alixWallet) }

        val directoryFile = File(context.filesDir.absolutePath, "testing_all")

        directoryFile.mkdirs()

        val allPath = directoryFile.absolutePath + "/testAll.zstd"

        val dm = runBlocking { alixClient.conversations.findOrCreateDm(boClient.inboxId) }
        runBlocking {
            dm.send("hi")
            alixClient.conversations.syncAllConversations()
            boClient.conversations.syncAllConversations()
        }
        val boDm = runBlocking { boClient.conversations.findDmByInboxId(alixClient.inboxId)!! }

        runBlocking { alixClient.createArchive(allPath, encryptionKey) }

        val alixClient2 = runBlocking { createClient(alixWallet) }

        runBlocking {
            alixClient2.importArchive(allPath, encryptionKey)
            alixClient2.conversations.syncAllConversations()
        }
        val convosList = runBlocking { alixClient2.conversations.list() }
        assertEquals(1, convosList.size)
        assertEquals(runBlocking { convosList.first().isActive() }, false)
        val dm2 = runBlocking { alixClient.conversations.findOrCreateDm(boClient.inboxId) }
        assertEquals(runBlocking { dm2.isActive() }, true)

        runBlocking {
            boDm.send("hey")
            dm2.send("hey")
            boClient.conversations.syncAllConversations()
            Thread.sleep(2000)
            alixClient2.conversations.syncAllConversations()
            val convosList2 = alixClient2.conversations.list()
            assertEquals(1, convosList2.size)
            assertEquals(dm2.messages().size, 4)
            assertEquals(boDm.messages().size, 4)
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

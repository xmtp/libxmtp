package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.libxmtp.ArchiveElement
import org.xmtp.android.library.libxmtp.ArchiveOptions
import org.xmtp.android.library.messages.PrivateKeyBuilder
import java.io.File
import java.security.SecureRandom

@RunWith(AndroidJUnit4::class)
class ArchiveTest {
    @Test
    fun testClientArchives() {
        val fixtures = fixtures()
        val key = SecureRandom().generateSeed(32)
        val encryptionKey = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()

        val alixClient = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    historySyncUrl = ""
                )
            )
        }

        val directoryFile = File(context.filesDir.absolutePath, "testing_all")
        val consentFile = File(context.filesDir.absolutePath, "testing_consent")

        directoryFile.mkdirs()
        consentFile.mkdirs()

        val allPath = directoryFile.absolutePath + "/testAll.zstd"
        val consentPath = consentFile.absolutePath + "/testConsent.zstd"

        val group = runBlocking { alixClient.conversations.newGroup(listOf(fixtures.boClient.inboxId)) }
        runBlocking {
            group.send("hi")
            alixClient.conversations.syncAllConversations()
            fixtures.boClient.conversations.syncAllConversations()
        }
        val boGroup = runBlocking { fixtures.boClient.conversations.findGroup(group.id)!! }

        runBlocking { alixClient.createArchive(allPath, encryptionKey) }
        runBlocking {
            alixClient.createArchive(
                consentPath,
                encryptionKey,
                opts = ArchiveOptions(archiveElements = listOf(ArchiveElement.CONSENT))
            )
        }

        val metadataAll = runBlocking { alixClient.archiveMetadata(allPath, encryptionKey) }
        val metadataConsent = runBlocking { alixClient.archiveMetadata(consentPath, encryptionKey) }

        assertEquals(metadataAll.elements.size, 2)
        assertEquals(metadataConsent.elements, listOf(ArchiveElement.CONSENT))

        val alixClient2 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = context.filesDir.absolutePath.toString()
                )
            )
        }

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
            fixtures.boClient.conversations.syncAllConversations()
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
        val fixtures = fixtures()
        val key = SecureRandom().generateSeed(32)
        val encryptionKey = SecureRandom().generateSeed(32)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val alixWallet = PrivateKeyBuilder()

        val alixClient = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key
                )
            )
        }

        val directoryFile = File(context.filesDir.absolutePath, "testing_all")

        directoryFile.mkdirs()

        val allPath = directoryFile.absolutePath + "/testAll.zstd"

        val dm = runBlocking { alixClient.conversations.findOrCreateDm(fixtures.boClient.inboxId) }
        runBlocking {
            dm.send("hi")
            alixClient.conversations.syncAllConversations()
            fixtures.boClient.conversations.syncAllConversations()
        }
        val boDm = runBlocking { fixtures.boClient.conversations.findDmByInboxId(alixClient.inboxId)!! }

        runBlocking { alixClient.createArchive(allPath, encryptionKey) }

        val alixClient2 = runBlocking {
            Client.create(
                account = alixWallet,
                options = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    appContext = context,
                    dbEncryptionKey = key,
                    dbDirectory = context.filesDir.absolutePath.toString()
                )
            )
        }

        runBlocking {
            alixClient2.importArchive(allPath, encryptionKey)
            alixClient2.conversations.syncAllConversations()
        }
        val convosList = runBlocking { alixClient2.conversations.list() }
        assertEquals(1, convosList.size)
        assertEquals(convosList.first().isActive(), false)
        val dm2 = runBlocking { alixClient.conversations.findOrCreateDm(fixtures.boClient.inboxId) }
        assertEquals(dm2.isActive(), true)

        runBlocking {
            boDm.send("hey")
            dm2.send("hey")
            fixtures.boClient.conversations.syncAllConversations()
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
        val fixtures = fixtures()
        val encryptionKey = SecureRandom().generateSeed(32)
        val directoryFile = File(fixtures.context.filesDir.absolutePath, "testing_all")

        directoryFile.mkdirs()

        val allPath = directoryFile.absolutePath + "/testAll.zstd"

        val group = runBlocking { fixtures.alixClient.conversations.newGroup(listOf(fixtures.boClient.inboxId)) }
        val dm = runBlocking { fixtures.alixClient.conversations.findOrCreateDm(fixtures.boClient.inboxId) }
        runBlocking {
            group.send("First")
            dm.send("hi")
            fixtures.alixClient.conversations.syncAllConversations()
            fixtures.boClient.conversations.syncAllConversations()
        }
        val boGroup = runBlocking { fixtures.boClient.conversations.findGroup(group.id)!! }

        assertEquals(runBlocking { group.messages() }.size, 2)
        assertEquals(runBlocking { boGroup.messages() }.size, 2)
        assertEquals(runBlocking { fixtures.alixClient.conversations.list() }.size, 2)
        assertEquals(runBlocking { fixtures.boClient.conversations.list() }.size, 2)

        runBlocking { fixtures.alixClient.createArchive(allPath, encryptionKey) }
        runBlocking { group.send("Second") }
        runBlocking { fixtures.alixClient.importArchive(allPath, encryptionKey) }
        runBlocking {
            group.send("Third")
            dm.send("hi")
            fixtures.alixClient.conversations.syncAllConversations()
            fixtures.boClient.conversations.syncAllConversations()
        }
        assertEquals(runBlocking { group.messages() }.size, 4)
        assertEquals(runBlocking { boGroup.messages() }.size, 4)
        assertEquals(runBlocking { fixtures.alixClient.conversations.list() }.size, 2)
        assertEquals(runBlocking { fixtures.boClient.conversations.list() }.size, 2)
    }
}

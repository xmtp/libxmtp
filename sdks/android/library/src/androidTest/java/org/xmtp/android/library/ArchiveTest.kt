package org.xmtp.android.library

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*
import java.io.File
import java.security.SecureRandom

class ArchiveTest : BaseInstrumentedTest() {
    private suspend fun findGroup(
        client: SDKClient,
        id: ConversationId,
    ): Group = (checkNotNull(client.conversations().getById(id)) as Conversation.Group).group

    private suspend fun findDm(
        client: SDKClient,
        id: ConversationId,
    ): Dm = (checkNotNull(client.conversations().getById(id)) as Conversation.Dm).dm

    @Test fun testClientArchives() =
        runBlocking {
            val fixtures = createFixtures()
            val key = SecureRandom().generateSeed(32)
            val signer = createWallet()
            val first = createClient(signer)
            val archive = File(testDbDir.newFolder(), "all.zstd").absolutePath
            val consent = File(testDbDir.newFolder(), "consent.zstd").absolutePath
            val group = first.conversations().createGroup(listOf(fixtures.boClient.inboxId()))
            group.sendText("hi")
            first.conversations().syncAll(null)
            fixtures.boClient.conversations().syncAll(null)
            val boGroup = findGroup(fixtures.boClient, group.id())
            first.archives().exportToFile(archive, key, null)
            first.archives().exportToFile(consent, key, ArchiveOptions(elements = listOf(ArchiveElement.CONSENT)))
            assertEquals(
                setOf(ArchiveElement.MESSAGES, ArchiveElement.CONSENT),
                first
                    .archives()
                    .metadataFromFile(archive, key)
                    .elements
                    .toSet(),
            )
            assertEquals(listOf(ArchiveElement.CONSENT), first.archives().metadataFromFile(consent, key).elements)

            val second = createClient(signer)
            second.archives().importFromFile(archive, key)
            first.conversations().syncAll(null)
            delay(2000)
            second.conversations().syncAll(null)
            first.preferences().sync()
            delay(2000)
            second.preferences().sync()
            boGroup.sendText("hey")
            fixtures.boClient.conversations().syncAll(null)
            second.conversations().syncAll(null)
            val conversations = second.conversations().list()
            assertEquals(1, conversations.size)
            val restored = (conversations.single() as Conversation.Group).group
            restored.sync()
            assertEquals(3, restored.messages().size)
            assertEquals(ConsentState.ALLOWED, restored.state().common.consentState)
        }

    @Test fun testInActiveDmsStitchIfDuplicated() =
        runBlocking {
            val fixtures = createFixtures()
            val key = SecureRandom().generateSeed(32)
            val signer = createWallet()
            val first = createClient(signer)
            val archive = File(testDbDir.newFolder(), "all.zstd").absolutePath
            val original = first.conversations().createDm(fixtures.boClient.inboxId())
            val archivedId = original.sendText("archived from alix")
            first.conversations().syncAll(null)
            fixtures.boClient.conversations().syncAll(null)
            val originalBo = checkNotNull(fixtures.boClient.conversations().getDmByInboxId(first.inboxId()))
            first.archives().exportToFile(archive, key, null)

            val second = createClient(signer)
            second.archives().importFromFile(archive, key)
            val restored = findDm(second, original.id())
            assertFalse(restored.state().isActive)
            assertEquals(1, second.conversations().listDms(null).size)
            val joined = second.conversations().createDm(fixtures.boClient.inboxId())
            assertNotEquals("The restored client must create a second physical DM", original.id(), joined.id())
            assertTrue(joined.state().isActive)
            assertEquals(1, second.conversations().listDms(null).size)
            val secondId = joined.sendText("live from alix installation 2")
            fixtures.boClient.conversations().syncAll(null)
            val joinedBo = findDm(fixtures.boClient, joined.id())
            assertEquals(joined.id(), joinedBo.id())
            val boId = joinedBo.sendText("reply from bo in the duplicate")
            second.conversations().syncAll(null)
            joined.sync()
            joinedBo.sync()
            originalBo.sync()
            assertEquals(1, second.conversations().listDms(null).size)

            data class Expected(
                val body: String,
                val sender: InboxId,
                val physicalGroup: ConversationId,
            )
            val expected =
                mapOf(
                    archivedId to Expected("archived from alix", first.inboxId(), original.id()),
                    secondId to Expected("live from alix installation 2", second.inboxId(), joined.id()),
                    boId to Expected("reply from bo in the duplicate", fixtures.boClient.inboxId(), joined.id()),
                )

            suspend fun assertStitched(
                label: String,
                conversation: Dm,
            ) {
                val messages = conversation.messages()
                val rawCount = conversation.countMessages(null)
                assertEquals(
                    "$label duplicate IDs; raw count=$rawCount",
                    messages.size,
                    messages.map { it.id }.toSet().size,
                )
                val applications = messages.filter { it.kind == MessageKind.APPLICATION }
                assertEquals("$label application IDs", expected.keys, applications.map { it.id }.toSet())
                assertEquals("$label application count", expected.size, applications.size)
                for (message in applications) {
                    val row = checkNotNull(expected[message.id])
                    val body = ((message.content as SDKMessageContent.Standard).value as MessageContent.Text).v1
                    assertEquals("$label body", row.body, body)
                    assertEquals("$label sender", row.sender, message.senderInboxId)
                    assertEquals("$label physical group", row.physicalGroup, message.conversationId)
                }
                assertEquals(
                    "$label includes both physical DMs",
                    setOf(original.id(), joined.id()),
                    applications
                        .map {
                            it.conversationId
                        }.toSet(),
                )
            }
            assertStitched("restored Alix DM", restored)
            assertStitched("joined Alix DM", joined)
            assertStitched("original Bo DM", originalBo)
            assertStitched("joined Bo DM", joinedBo)
        }

    @Test fun testImportArchiveWorksEvenOnFullDatabase() =
        runBlocking {
            val fixtures = createFixtures()
            val alix = fixtures.alixClient
            val bo = fixtures.boClient
            val key = SecureRandom().generateSeed(32)
            val archive = File(testDbDir.newFolder(), "all.zstd").absolutePath
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            val dm = alix.conversations().createDm(bo.inboxId())
            group.sendText("First")
            dm.sendText("hi")
            alix.conversations().syncAll(null)
            bo.conversations().syncAll(null)
            val boGroup = findGroup(bo, group.id())
            assertEquals(2, group.messages().size)
            assertEquals(2, boGroup.messages().size)
            assertEquals(2, alix.conversations().list().size)
            assertEquals(2, bo.conversations().list().size)
            alix.archives().exportToFile(archive, key, null)
            group.sendText("Second")
            alix.archives().importFromFile(archive, key)
            group.sendText("Third")
            dm.sendText("hi")
            alix.conversations().syncAll(null)
            bo.conversations().syncAll(null)
            assertEquals(4, group.messages().size)
            assertEquals(4, boGroup.messages().size)
            assertEquals(2, alix.conversations().list().size)
            assertEquals(2, bo.conversations().list().size)
        }
}

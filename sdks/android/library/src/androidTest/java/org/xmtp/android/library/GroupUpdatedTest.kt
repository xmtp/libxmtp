package org.xmtp.android.library

import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import uniffi.xmtp_sdk.*

class GroupUpdatedTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private val alix get() = fixtures.alixClient
    private val bo get() = fixtures.boClient
    private val caro get() = fixtures.caroClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
    }

    private fun updateOrNull(message: Message): GroupUpdated? =
        ((message.content as? SDKMessageContent.Standard)?.value as? MessageContent.GroupUpdated)?.v1

    private fun update(message: Message): GroupUpdated =
        ((message.content as SDKMessageContent.Standard).value as MessageContent.GroupUpdated).v1

    @Test fun testCanAddMembers() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId(), caro.inboxId()))
            val messages = group.messages()
            assertEquals(1, messages.size)
            val value = update(messages.single())
            assertEquals(listOf(bo.inboxId(), caro.inboxId()).sorted(), value.addedInboxes.sorted())
            assertTrue(value.removedInboxes.isEmpty())
        }

    @Test fun testCanRemoveMembers() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId(), caro.inboxId()))
            assertEquals(1, group.messages().size)
            assertEquals(3, group.members().size)
            group.removeMembers(listOf(caro.inboxId()))
            assertEquals(2, group.messages().size)
            assertEquals(2, group.members().size)
            val value = update(group.messages(ListMessagesOptions(direction = MessageOrder.DESCENDING)).first())
            assertEquals(listOf(caro.inboxId()), value.removedInboxes)
            assertTrue(value.addedInboxes.isEmpty())
        }

    @Test fun testRemovesInvalidMessageKind() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId(), caro.inboxId()))
            val value =
                GroupUpdated(
                    alix.inboxId(),
                    emptyList(),
                    emptyList(),
                    emptyList(),
                    emptyList(),
                    emptyList(),
                    emptyList(),
                    emptyList(),
                    emptyList(),
                )
            val error =
                assertThrows(XmtpException.InvalidInput::class.java) {
                    runBlocking { group.send(GroupUpdatedCodec().encode(value)) }
                }
            assertEquals("ReservedTranscriptContentType", error.v1.code)
            group.sync()
            assertEquals(1, group.messages().size)
            assertEquals(3, group.members().size)
        }

    @Test fun testIfNotRegisteredReturnsFallback() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId(), caro.inboxId()))
            assertEquals(1, group.messages().size)
            assertTrue(
                group
                    .messages()
                    .single()
                    .fallback
                    .orEmpty()
                    .isBlank(),
            )
        }

    @Test fun testCanUpdateGroupName() =
        runBlocking {
            val group =
                alix.conversations().createGroup(
                    listOf(bo.inboxId(), caro.inboxId()),
                    CreateGroupOptions(name = "Start Name"),
                )
            assertEquals(1, group.messages().size)
            group.updateName("Group Name")
            assertEquals(2, group.messages().size)
            val value = update(group.messages().first())
            assertEquals("Start Name", value.metadataFieldChanges.first().oldValue)
            assertEquals("Group Name", value.metadataFieldChanges.first().newValue)
            val debug = group.debugInfo()
            assertEquals(2uL, debug.epoch)
            assertFalse(debug.maybeForked)
            assertEquals("", debug.forkDetails)
        }

    private suspend fun leave(group: Group): GroupUpdated =
        withTimeout(10_000) {
            bo.conversations().sync()
            bo
                .conversations()
                .listGroups(null)
                .single { it.id() == group.id() }
                .requestRemoval()
            var value: GroupUpdated? = null
            while (value == null) {
                group.sync()
                value = group.messages().mapNotNull(::updateOrNull).firstOrNull { bo.inboxId() in it.leftInboxes }
                if (value == null) delay(50)
            }
            value
        }

    @Test fun testLeftInboxesPopulatedWhenMemberLeaves() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            val value = leave(group)
            assertEquals(listOf(bo.inboxId()), value.leftInboxes)
            assertTrue(value.removedInboxes.isEmpty())
        }

    @Test fun testLeftInboxesPersistedAfterClientReinitialization() =
        runBlocking {
            val group = alix.conversations().createGroup(listOf(bo.inboxId()))
            val groupId = group.id()
            val inboxId = alix.inboxId()
            val value = leave(group)
            assertEquals(listOf(bo.inboxId()), value.leftInboxes)
            val options = alix.options().let { it.copy(storage = it.storage.copy(encryptionKey = dbEncryptionKey)) }
            alix.end()
            val reopened = trackClient(SDKClient.build(context, fixtures.alix, options))
            assertEquals(inboxId, reopened.inboxId())
            val restored = reopened.conversations().listGroups(null).single { it.id() == groupId }
            val retained = restored.messages().mapNotNull(::updateOrNull).single { bo.inboxId() in it.leftInboxes }
            assertEquals(listOf(bo.inboxId()), retained.leftInboxes)
            assertTrue(retained.removedInboxes.isEmpty())
            reopened.end()
        }
}

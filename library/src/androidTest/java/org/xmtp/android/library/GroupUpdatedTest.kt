package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.GroupUpdated
import org.xmtp.android.library.codecs.GroupUpdatedCodec

@RunWith(AndroidJUnit4::class)
class GroupUpdatedTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client
    private lateinit var caroClient: Client

    @Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
        Client.register(codec = GroupUpdatedCodec())
    }

    @Test
    fun testCanAddMembers() {
        val group =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId, caroClient.inboxId))
            }
        val messages = runBlocking { group.messages() }
        assertEquals(messages.size, 1)
        val content: GroupUpdated? = messages.first().content()
        assertEquals(
            listOf(boClient.inboxId, caroClient.inboxId).sorted(),
            content?.addedInboxesList?.map { it.inboxId }?.sorted(),
        )
        assert(content?.removedInboxesList.isNullOrEmpty())
    }

    @Test
    fun testCanRemoveMembers() {
        val group =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId, caroClient.inboxId))
            }
        val messages = runBlocking { group.messages() }
        assertEquals(messages.size, 1)
        assertEquals(runBlocking { group.members().size }, 3)
        runBlocking { group.removeMembers(listOf(caroClient.inboxId)) }
        val updatedMessages = runBlocking { group.messages() }
        assertEquals(updatedMessages.size, 2)
        assertEquals(runBlocking { group.members().size }, 2)
        val content: GroupUpdated? = updatedMessages.first().content()

        assertEquals(
            listOf(caroClient.inboxId),
            content?.removedInboxesList?.map { it.inboxId }?.sorted(),
        )
        assert(content?.addedInboxesList.isNullOrEmpty())
    }

    @Test
    fun testRemovesInvalidMessageKind() {
        val membershipChange = GroupUpdated.newBuilder().build()

        val group =
            runBlocking {
                alixClient.conversations.newGroup(listOf(boClient.inboxId, caroClient.inboxId))
            }
        val messages = runBlocking { group.messages() }
        assertEquals(messages.size, 1)
        assertEquals(runBlocking { group.members().size }, 3)
        runBlocking {
            group.send(
                content = membershipChange,
                options = SendOptions(contentType = ContentTypeGroupUpdated),
            )
            group.sync()
        }
        val updatedMessages = runBlocking { group.messages() }
        assertEquals(updatedMessages.size, 1)
    }

    @Test
    fun testIfNotRegisteredReturnsFallback() =
        runBlocking {
            val group = alixClient.conversations.newGroup(listOf(boClient.inboxId, caroClient.inboxId))
            val messages = group.messages()
            assertEquals(messages.size, 1)
            assert(messages.first().fallback.isBlank())
        }

    @Test
    fun testCanUpdateGroupName() {
        val group =
            runBlocking {
                alixClient.conversations.newGroup(
                    listOf(boClient.inboxId, caroClient.inboxId),
                    groupName = "Start Name",
                )
            }
        var messages = runBlocking { group.messages() }
        assertEquals(messages.size, 1)
        runBlocking {
            group.updateName("Group Name")
            messages = group.messages()
            assertEquals(messages.size, 2)

            val content: GroupUpdated? = messages.first().content()
            assertEquals("Start Name", content?.metadataFieldChangesList?.first()?.oldValue)
            assertEquals("Group Name", content?.metadataFieldChangesList?.first()?.newValue)
        }
        runBlocking {
            assertEquals(group.getDebugInformation().epoch, 2)
            assertEquals(group.getDebugInformation().maybeForked, false)
            assertEquals(group.getDebugInformation().forkDetails, "")
        }
    }
}

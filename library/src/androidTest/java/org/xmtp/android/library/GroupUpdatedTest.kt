package org.xmtp.android.library

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeGroupUpdated
import org.xmtp.android.library.codecs.GroupUpdated
import org.xmtp.android.library.codecs.GroupUpdatedCodec
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class GroupUpdatedTest {
    lateinit var alixWallet: PrivateKeyBuilder
    lateinit var boWallet: PrivateKeyBuilder
    lateinit var alix: PrivateKey
    lateinit var alixClient: Client
    lateinit var bo: PrivateKey
    lateinit var boClient: Client
    lateinit var caroWallet: PrivateKeyBuilder
    lateinit var caro: PrivateKey
    lateinit var caroClient: Client
    lateinit var fixtures: Fixtures
    val context = ApplicationProvider.getApplicationContext<Context>()

    @Before
    fun setUp() {
        fixtures = fixtures(
            clientOptions = ClientOptions(
                ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                enableV3 = true,
                appContext = context,
            )
        )
        alixWallet = fixtures.aliceAccount
        alix = fixtures.alice
        boWallet = fixtures.bobAccount
        bo = fixtures.bob
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        alixClient = fixtures.aliceClient
        boClient = fixtures.bobClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testCanAddMembers() {
        Client.register(codec = GroupUpdatedCodec())

        val group = runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    bo.walletAddress,
                    caro.walletAddress
                )
            )
        }
        val messages = group.messages()
        assertEquals(messages.size, 1)
        val content: GroupUpdated? = messages.first().content()
        assertEquals(
            listOf(boClient.inboxId, caroClient.inboxId).sorted(),
            content?.addedInboxesList?.map { it.inboxId }?.sorted()
        )
        assert(content?.removedInboxesList.isNullOrEmpty())
    }

    @Test
    fun testCanRemoveMembers() {
        Client.register(codec = GroupUpdatedCodec())

        val group = runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    bo.walletAddress,
                    caro.walletAddress
                )
            )
        }
        val messages = group.messages()
        assertEquals(messages.size, 1)
        assertEquals(group.members().size, 3)
        runBlocking { group.removeMembers(listOf(caro.walletAddress)) }
        val updatedMessages = group.messages()
        assertEquals(updatedMessages.size, 2)
        assertEquals(group.members().size, 2)
        val content: GroupUpdated? = updatedMessages.first().content()

        assertEquals(
            listOf(caroClient.inboxId),
            content?.removedInboxesList?.map { it.inboxId }?.sorted()
        )
        assert(content?.addedInboxesList.isNullOrEmpty())
    }

    @Test
    fun testRemovesInvalidMessageKind() {
        Client.register(codec = GroupUpdatedCodec())

        val membershipChange = GroupUpdated.newBuilder().build()

        val group = runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    bo.walletAddress,
                    caro.walletAddress
                )
            )
        }
        val messages = group.messages()
        assertEquals(messages.size, 1)
        assertEquals(group.members().size, 3)
        runBlocking {
            group.send(
                content = membershipChange,
                options = SendOptions(contentType = ContentTypeGroupUpdated),
            )
            group.sync()
        }
        val updatedMessages = group.messages()
        assertEquals(updatedMessages.size, 1)
    }

    @Test
    fun testIfNotRegisteredReturnsFallback() {
        val group = runBlocking {
            alixClient.conversations.newGroup(
                listOf(
                    bo.walletAddress,
                    caro.walletAddress
                )
            )
        }
        val messages = group.messages()
        assertEquals(messages.size, 1)
        assert(messages.first().fallbackContent.isBlank())
    }
}

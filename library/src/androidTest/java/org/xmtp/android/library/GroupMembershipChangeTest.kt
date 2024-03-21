package org.xmtp.android.library

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChangeCodec
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChanges

@RunWith(AndroidJUnit4::class)
class GroupMembershipChangeTest {
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
                enableAlphaMls = true,
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
        Client.register(codec = GroupMembershipChangeCodec())

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
        val content: GroupMembershipChanges? = messages.first().content()
        assertEquals(
            listOf(bo.walletAddress.lowercase(), caro.walletAddress.lowercase()).sorted(),
            content?.membersAddedList?.map { it.accountAddress.lowercase() }?.sorted()
        )
        assert(content?.membersRemovedList.isNullOrEmpty())
    }

    @Test
    fun testCanRemoveMembers() {
        Client.register(codec = GroupMembershipChangeCodec())

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
        assertEquals(group.memberAddresses().size, 3)
        runBlocking { group.removeMembers(listOf(caro.walletAddress)) }
        val updatedMessages = group.messages()
        assertEquals(updatedMessages.size, 2)
        assertEquals(group.memberAddresses().size, 2)
        val content: GroupMembershipChanges? = updatedMessages.first().content()

        assertEquals(
            listOf(caro.walletAddress.lowercase()),
            content?.membersRemovedList?.map { it.accountAddress.lowercase() }?.sorted()
        )
        assert(content?.membersAddedList.isNullOrEmpty())
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

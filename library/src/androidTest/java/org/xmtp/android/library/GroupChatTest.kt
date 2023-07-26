package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeGroupChatMemberAdded
import org.xmtp.android.library.codecs.ContentTypeGroupTitleChangedAdded
import org.xmtp.android.library.codecs.GroupChatMemberAdded
import org.xmtp.android.library.codecs.GroupChatTitleChanged
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class GroupChatTest {
    @Test
    fun testCanAddMemberToGroupChatCodec() {
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        aliceClient.enableGroupChat()
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)

        val personAdded = GroupChatMemberAdded(
            member = fixtures.steve.walletAddress,
        )

        aliceConversation.send(
            content = personAdded,
            options = SendOptions(contentType = ContentTypeGroupChatMemberAdded),
        )
        val messages = aliceConversation.messages()
        assertEquals(messages.size, 1)
        val content: GroupChatMemberAdded? = messages[0].content()
        assertEquals(fixtures.steve.walletAddress, content?.member)
    }

    @Test
    fun testCanChangeGroupChatNameCodec() {
        val fixtures = fixtures()
        val aliceClient = fixtures.aliceClient
        aliceClient.enableGroupChat()
        val aliceConversation =
            aliceClient.conversations.newConversation(fixtures.bob.walletAddress)

        val titleChange = GroupChatTitleChanged(
            newTitle = "New Title",
        )

        aliceConversation.send(
            content = titleChange,
            options = SendOptions(contentType = ContentTypeGroupTitleChangedAdded),
        )
        val messages = aliceConversation.messages()
        assertEquals(messages.size, 1)
        val content: GroupChatTitleChanged? = messages[0].content()
        assertEquals("New Title", content?.newTitle)
    }
}

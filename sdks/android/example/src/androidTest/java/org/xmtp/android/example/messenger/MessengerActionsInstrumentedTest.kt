package org.xmtp.android.example.messenger

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.security.SecureRandom
import java.util.UUID
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class MessengerActionsInstrumentedTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
    private suspend fun until(check: suspend () -> Boolean) { withTimeout(30_000) { while (!check()) delay(50) } }
    @Test fun acceptedIdTextReplyReactionRetryAndDeleteUseOneStoredRow() = runBlocking {
        AndroidStreamLifecycle.enabled = false
        val session = AppSession(context)
        var peer: SDKClient? = null
        var peerReader: Job? = null
        try {
            session.connect(BuildConfig.XMTP_BACKEND_URL, "", true)
            val owner = checkNotNull(session.active.value)
            peer = SDKClient.create(context, localSignerFromPrivateKey(SecureRandom().generateSeed(32)), ClientOptions(backend = BackendSource.Options(BackendOptions(url = BuildConfig.XMTP_BACKEND_URL)), storage = StorageOptions(location = StorageLocation.InMemory), deviceSync = false))
            val second = checkNotNull(peer)
            peerReader = launch { second.conversations.streamAllMessages().collect { } }
            val chat = Conversation.Group(owner.client.conversations.createGroup(listOf(second.inboxId()), CreateGroupOptions(name = "Before", permissions = GroupPermissionMode.AllMembers)))
            val coordinator = SendCoordinator(session.preferences, session::accepts)
            var queues = 0
            val id = coordinator.queue(owner.key, owner.client, chat, reconcile = {}) { queues += 1; chat.sendText("one text", SendOptions(optimistic = true)) }
            until { owner.client.conversations.getMessageById(id)?.deliveryStatus == DeliveryStatus.PUBLISHED }
            coordinator.retry(owner.key, owner.client, chat, id) { }
            assertEquals(1, queues)
            assertEquals(1, chat.messages().count { it.id == id })
            val parent = checkNotNull(owner.client.conversations.getMessageById(id))
            val reply = coordinator.queue(owner.key, owner.client, chat, reconcile = {}) { parent.reply("one reply", SendOptions(optimistic = true)) }
            val reaction = coordinator.queue(owner.key, owner.client, chat, reconcile = {}) { parent.react(Reaction("👍", ReactionAction.ADDED, ReactionSchema.UNICODE), SendOptions(optimistic = true)) }
            until { checkNotNull(owner.client.conversations.getMessageById(id)).reactions.any { it.reaction.content == "👍" } }
            coordinator.queue(owner.key, owner.client, chat, reconcile = {}) { parent.react(Reaction("👍", ReactionAction.REMOVED, ReactionSchema.UNICODE), SendOptions(optimistic = true)) }
            until { checkNotNull(owner.client.conversations.getMessageById(id)).reactions.isEmpty() }
            assertEquals(id, owner.client.conversations.getMessageById(reply)?.inReplyTo?.id)
            val group = (chat as Conversation.Group).group
            group.updateName("After"); group.updateDescription("Current description")
            assertEquals("After", group.state().name)
            group.addAdmin(second.inboxId()); assertTrue(group.state().admins.contains(second.inboxId()))
            group.removeAdmin(second.inboxId()); assertFalse(group.state().admins.contains(second.inboxId()))
            applyStandardPreset(group, true)
            assertEquals(GroupPolicyType.ADMIN_ONLY, group.state().permissions.policyType)
            applyStandardPreset(group, false)
            assertEquals(GroupPolicyType.ALL_MEMBERS, group.state().permissions.policyType)
            group.updateDisappearingSettings(DisappearingSettings(Timestamp(System.currentTimeMillis() * 1_000_000), 1_000_000_000L))
            assertTrue(group.state().common.isDisappearingEnabled)
            group.updateDisappearingSettings(null)
            chat.deleteMessage(id)
            until { owner.client.conversations.getMessageById(id)?.toRow(owner.client.inboxId())?.deleted == true }
            assertEquals("Message deleted", owner.client.conversations.getMessageById(id)?.toRow(owner.client.inboxId())?.text)
        } finally {
            peerReader?.cancelAndJoin()
            withContext(NonCancellable) { peer?.end(); session.deleteAccount() }
            AndroidStreamLifecycle.enabled = true
        }
    }
}

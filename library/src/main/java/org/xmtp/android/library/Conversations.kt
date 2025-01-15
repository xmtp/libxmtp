package org.xmtp.android.library

import android.util.Log
import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import org.xmtp.android.library.libxmtp.GroupPermissionPreconfiguration
import org.xmtp.android.library.libxmtp.Message
import org.xmtp.android.library.messages.Topic
import org.xmtp.proto.keystore.api.v1.Keystore
import org.xmtp.android.library.libxmtp.PermissionPolicySet
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiConversationListItem
import uniffi.xmtpv3.FfiConversationType
import uniffi.xmtpv3.FfiConversations
import uniffi.xmtpv3.FfiCreateGroupOptions
import uniffi.xmtpv3.FfiGroupPermissionsOptions
import uniffi.xmtpv3.FfiListConversationsOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiPermissionPolicySet
import uniffi.xmtpv3.FfiSubscribeException
import java.util.Date
import kotlin.time.Duration.Companion.nanoseconds
import kotlin.time.DurationUnit

data class Conversations(
    var client: Client,
    private val ffiConversations: FfiConversations,
) {

    enum class ConversationType {
        ALL,
        GROUPS,
        DMS;
    }

    suspend fun fromWelcome(envelopeBytes: ByteArray): Conversation {
        val conversation = ffiConversations.processStreamedWelcomeMessage(envelopeBytes)
        return when (conversation.conversationType()) {
            FfiConversationType.DM -> Conversation.Dm(Dm(client.inboxId, conversation))
            else -> Conversation.Group(Group(client.inboxId, conversation))
        }
    }

    suspend fun newGroup(
        accountAddresses: List<String>,
        permissions: GroupPermissionPreconfiguration = GroupPermissionPreconfiguration.ALL_MEMBERS,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        groupPinnedFrameUrl: String = "",
        messageExpirationFromMs: Long? = null,
        messageExpirationMs: Long? = null,
    ): Group {
        return newGroupInternal(
            accountAddresses,
            GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(permissions),
            groupName,
            groupImageUrlSquare,
            groupDescription,
            groupPinnedFrameUrl,
            null,
            messageExpirationFromMs,
            messageExpirationMs,
        )
    }

    suspend fun newGroupCustomPermissions(
        accountAddresses: List<String>,
        permissionPolicySet: PermissionPolicySet,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        groupPinnedFrameUrl: String = "",
        messageExpirationFromMs: Long? = null,
        messageExpirationMs: Long? = null,
    ): Group {
        return newGroupInternal(
            accountAddresses,
            FfiGroupPermissionsOptions.CUSTOM_POLICY,
            groupName,
            groupImageUrlSquare,
            groupDescription,
            groupPinnedFrameUrl,
            PermissionPolicySet.toFfiPermissionPolicySet(permissionPolicySet),
            messageExpirationFromMs,
            messageExpirationMs
        )
    }

    private suspend fun newGroupInternal(
        accountAddresses: List<String>,
        permissions: FfiGroupPermissionsOptions,
        groupName: String,
        groupImageUrlSquare: String,
        groupDescription: String,
        groupPinnedFrameUrl: String,
        permissionsPolicySet: FfiPermissionPolicySet?,
        messageExpirationFromMs: Long?,
        messageExpirationMs: Long?,
    ): Group {
        if (accountAddresses.size == 1 &&
            accountAddresses.first().lowercase() == client.address.lowercase()
        ) {
            throw XMTPException("Recipient is sender")
        }
        val falseAddresses =
            if (accountAddresses.isNotEmpty()) client.canMessage(accountAddresses)
                .filter { !it.value }.map { it.key } else emptyList()
        if (falseAddresses.isNotEmpty()) {
            throw XMTPException("${falseAddresses.joinToString()} not on network")
        }

        val group =
            ffiConversations.createGroup(
                accountAddresses,
                opts = FfiCreateGroupOptions(
                    permissions = permissions,
                    groupName = groupName,
                    groupImageUrlSquare = groupImageUrlSquare,
                    groupDescription = groupDescription,
                    groupPinnedFrameUrl = groupPinnedFrameUrl,
                    customPermissionPolicySet = permissionsPolicySet,
                    messageExpirationFromMs = messageExpirationFromMs,
                    messageExpirationMs = messageExpirationMs,
                )
            )
        return Group(client.inboxId, group)
    }

    // Sync from the network the latest list of conversations
    suspend fun sync() {
        ffiConversations.sync()
    }

    // Sync all new and existing conversations data from the network
    suspend fun syncAllConversations(consentState: ConsentState? = null): UInt {
        return ffiConversations.syncAllConversations(
            consentState?.let {
                ConsentState.toFfiConsentState(it)
            }
        )
    }

    suspend fun newConversation(peerAddress: String): Conversation {
        val dm = findOrCreateDm(peerAddress)
        return Conversation.Dm(dm)
    }

    suspend fun findOrCreateDm(peerAddress: String): Dm {
        if (peerAddress.lowercase() == client.address.lowercase()) {
            throw XMTPException("Recipient is sender")
        }
        val falseAddresses =
            client.canMessage(listOf(peerAddress)).filter { !it.value }.map { it.key }
        if (falseAddresses.isNotEmpty()) {
            throw XMTPException("${falseAddresses.joinToString()} not on network")
        }
        var dm = client.findDmByAddress(peerAddress)
        if (dm == null) {
            val dmConversation = ffiConversations.createDm(peerAddress.lowercase())
            dm = Dm(client.inboxId, dmConversation)
        }
        return dm
    }

    fun listGroups(
        after: Date? = null,
        before: Date? = null,
        limit: Int? = null,
        consentState: ConsentState? = null,
    ): List<Group> {
        val ffiGroups = ffiConversations.listGroups(
            opts = FfiListConversationsOptions(
                after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit?.toLong(),
                consentState?.let { ConsentState.toFfiConsentState(it) },
                false
            )
        )

        return ffiGroups.map {
            Group(client.inboxId, it.conversation(), it.lastMessage())
        }
    }

    fun listDms(
        after: Date? = null,
        before: Date? = null,
        limit: Int? = null,
        consentState: ConsentState? = null,
    ): List<Dm> {
        val ffiDms = ffiConversations.listDms(
            opts = FfiListConversationsOptions(
                after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit?.toLong(),
                consentState?.let { ConsentState.toFfiConsentState(it) },
                false
            )
        )

        return ffiDms.map {
            Dm(client.inboxId, it.conversation(), it.lastMessage())
        }
    }

    suspend fun list(
        after: Date? = null,
        before: Date? = null,
        limit: Int? = null,
        consentState: ConsentState? = null,
    ): List<Conversation> {
        val ffiConversation = ffiConversations.list(
            FfiListConversationsOptions(
                after?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                before?.time?.nanoseconds?.toLong(DurationUnit.NANOSECONDS),
                limit?.toLong(),
                consentState?.let { ConsentState.toFfiConsentState(it) },
                false
            )
        )

        return ffiConversation.map { it.toConversation() }
    }

    private suspend fun FfiConversationListItem.toConversation(): Conversation {
        return when (conversation().conversationType()) {
            FfiConversationType.DM -> Conversation.Dm(Dm(client.inboxId, conversation(), lastMessage()))
            else -> Conversation.Group(Group(client.inboxId, conversation(), lastMessage()))
        }
    }

    fun stream(type: ConversationType = ConversationType.ALL): Flow<Conversation> =
        callbackFlow {
            val conversationCallback = object : FfiConversationCallback {
                override fun onConversation(conversation: FfiConversation) {
                    launch(Dispatchers.IO) {
                        when (conversation.conversationType()) {
                            FfiConversationType.DM -> trySend(
                                Conversation.Dm(
                                    Dm(
                                        client.inboxId,
                                        conversation
                                    )
                                )
                            )

                            else -> trySend(Conversation.Group(Group(client.inboxId, conversation)))
                        }
                    }
                }

                override fun onError(error: FfiSubscribeException) {
                    Log.e("XMTP Conversation stream", error.message.toString())
                }
            }

            val stream = when (type) {
                ConversationType.ALL -> ffiConversations.stream(conversationCallback)
                ConversationType.GROUPS -> ffiConversations.streamGroups(conversationCallback)
                ConversationType.DMS -> ffiConversations.streamDms(conversationCallback)
            }

            awaitClose { stream.end() }
        }

    fun streamAllMessages(type: ConversationType = ConversationType.ALL): Flow<Message> =
        callbackFlow {
            val messageCallback = object : FfiMessageCallback {
                override fun onMessage(message: FfiMessage) {
                    val decodedMessage = Message.create(message)
                    decodedMessage?.let { trySend(it) }
                }

                override fun onError(error: FfiSubscribeException) {
                    Log.e("XMTP all message stream", error.message.toString())
                }
            }

            val stream = when (type) {
                ConversationType.ALL -> ffiConversations.streamAllMessages(messageCallback)
                ConversationType.GROUPS -> ffiConversations.streamAllGroupMessages(messageCallback)
                ConversationType.DMS -> ffiConversations.streamAllDmMessages(messageCallback)
            }

            awaitClose { stream.end() }
        }

    fun getHmacKeys(): Keystore.GetConversationHmacKeysResponse {
        val hmacKeysResponse = Keystore.GetConversationHmacKeysResponse.newBuilder()
        val conversations = ffiConversations.getHmacKeys()
        conversations.iterator().forEach {
            val hmacKeys = Keystore.GetConversationHmacKeysResponse.HmacKeys.newBuilder()
            it.value.forEach { key ->
                val hmacKeyData = Keystore.GetConversationHmacKeysResponse.HmacKeyData.newBuilder()
                hmacKeyData.hmacKey = key.key.toByteString()
                hmacKeyData.thirtyDayPeriodsSinceEpoch = key.epoch.toInt()
                hmacKeys.addValues(hmacKeyData)
            }
            hmacKeysResponse.putHmacKeys(
                Topic.groupMessage(it.key.toHex()).description,
                hmacKeys.build()
            )
        }
        return hmacKeysResponse.build()
    }
}

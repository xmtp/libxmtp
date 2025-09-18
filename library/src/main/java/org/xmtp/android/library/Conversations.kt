package org.xmtp.android.library

import android.util.Log
import com.google.protobuf.kotlin.toByteString
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.launch
import org.xmtp.android.library.libxmtp.DecodedMessage
import org.xmtp.android.library.libxmtp.DecodedMessageV2
import org.xmtp.android.library.libxmtp.DisappearingMessageSettings
import org.xmtp.android.library.libxmtp.GroupPermissionPreconfiguration
import org.xmtp.android.library.libxmtp.PermissionPolicySet
import org.xmtp.android.library.libxmtp.PublicIdentity
import org.xmtp.proto.keystore.api.v1.Keystore
import uniffi.xmtpv3.FfiConversation
import uniffi.xmtpv3.FfiConversationCallback
import uniffi.xmtpv3.FfiConversationListItem
import uniffi.xmtpv3.FfiConversationType
import uniffi.xmtpv3.FfiConversations
import uniffi.xmtpv3.FfiCreateDmOptions
import uniffi.xmtpv3.FfiCreateGroupOptions
import uniffi.xmtpv3.FfiGroupPermissionsOptions
import uniffi.xmtpv3.FfiGroupQueryOrderBy
import uniffi.xmtpv3.FfiListConversationsOptions
import uniffi.xmtpv3.FfiMessage
import uniffi.xmtpv3.FfiMessageCallback
import uniffi.xmtpv3.FfiMessageDisappearingSettings
import uniffi.xmtpv3.FfiPermissionPolicySet
import uniffi.xmtpv3.FfiSubscribeException
import uniffi.xmtpv3.FfiXmtpClient

data class Conversations(
    var client: Client,
    private val ffiConversations: FfiConversations,
    private val ffiClient: FfiXmtpClient,
) {

    enum class ConversationFilterType {
        ALL,
        GROUPS,
        DMS;
    }

    enum class ListConversationsOrderBy {
        CREATED_AT,
        LAST_ACTIVITY;
    }

    fun ListConversationsOrderBy.toFfi(): FfiGroupQueryOrderBy {
        return when (this) {
            ListConversationsOrderBy.CREATED_AT -> FfiGroupQueryOrderBy.CREATED_AT
            ListConversationsOrderBy.LAST_ACTIVITY -> FfiGroupQueryOrderBy.LAST_ACTIVITY
        }
    }

    fun findGroup(groupId: String): Group? {
        return try {
            Group(client, ffiClient.conversation(groupId.hexToByteArray()))
        } catch (e: Exception) {
            null
        }
    }

    suspend fun findConversation(conversationId: String): Conversation? {
        return try {
            val conversation = ffiClient.conversation(conversationId.hexToByteArray())
            when (conversation.conversationType()) {
                FfiConversationType.GROUP -> Conversation.Group(Group(client, conversation))
                FfiConversationType.DM -> Conversation.Dm(Dm(client, conversation))
                else -> null
            }
        } catch (e: Exception) {
            null
        }
    }

    suspend fun findConversationByTopic(topic: String): Conversation? {
        val regex = """/xmtp/mls/1/g-(.*?)/proto""".toRegex()
        val matchResult = regex.find(topic)
        val conversationId = matchResult?.groupValues?.get(1) ?: ""
        return try {
            val conversation = ffiClient.conversation(conversationId.hexToByteArray())
            when (conversation.conversationType()) {
                FfiConversationType.GROUP -> Conversation.Group(Group(client, conversation))
                FfiConversationType.DM -> Conversation.Dm(Dm(client, conversation))
                else -> null
            }
        } catch (e: Exception) {
            null
        }
    }

    fun findDmByInboxId(inboxId: InboxId): Dm? {
        return try {
            Dm(client, ffiClient.dmConversation(inboxId))
        } catch (e: Exception) {
            null
        }
    }

    suspend fun findDmByIdentity(publicIdentity: PublicIdentity): Dm? {
        val inboxId =
            client.inboxIdFromIdentity(publicIdentity)
                ?: throw XMTPException("No inboxId present")
        return findDmByInboxId(inboxId)
    }

    fun findMessage(messageId: String): DecodedMessage? {
        return try {
            DecodedMessage.create(ffiClient.message(messageId.hexToByteArray()))
        } catch (e: Exception) {
            null
        }
    }

    fun findEnrichedMessage(messageId: String): DecodedMessageV2? {
        return try {
            DecodedMessageV2.create(ffiClient.messageV2(messageId.hexToByteArray()))
        } catch (e: Exception) {
            Log.e("findEnrichedMessage failed", e.toString())
            null
        }
    }

    suspend fun fromWelcome(envelopeBytes: ByteArray): Conversation {
        val conversation = ffiConversations.processStreamedWelcomeMessage(envelopeBytes)
        return when (conversation.conversationType()) {
            FfiConversationType.DM -> Conversation.Dm(Dm(client, conversation))
            else -> Conversation.Group(Group(client, conversation))
        }
    }

    suspend fun newGroupWithIdentities(
        identities: List<PublicIdentity>,
        permissions: GroupPermissionPreconfiguration = GroupPermissionPreconfiguration.ALL_MEMBERS,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Group {
        return newGroupInternalWithIdentities(
            identities,
            GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(permissions),
            groupName,
            groupImageUrlSquare,
            groupDescription,
            null,
            disappearingMessageSettings?.let {
                FfiMessageDisappearingSettings(
                    it.disappearStartingAtNs,
                    it.retentionDurationInNs
                )
            },
        )
    }

    suspend fun newGroupCustomPermissionsWithIdentities(
        identities: List<PublicIdentity>,
        permissionPolicySet: PermissionPolicySet,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Group {
        return newGroupInternalWithIdentities(
            identities,
            FfiGroupPermissionsOptions.CUSTOM_POLICY,
            groupName,
            groupImageUrlSquare,
            groupDescription,
            PermissionPolicySet.toFfiPermissionPolicySet(permissionPolicySet),
            disappearingMessageSettings?.let {
                FfiMessageDisappearingSettings(
                    it.disappearStartingAtNs,
                    it.retentionDurationInNs
                )
            }
        )
    }

    private suspend fun newGroupInternalWithIdentities(
        identities: List<PublicIdentity>,
        permissions: FfiGroupPermissionsOptions,
        groupName: String,
        groupImageUrlSquare: String,
        groupDescription: String,
        permissionsPolicySet: FfiPermissionPolicySet?,
        messageDisappearingSettings: FfiMessageDisappearingSettings?,
    ): Group {
        val group =
            ffiConversations.createGroup(
                identities.map { it.ffiPrivate },
                opts = FfiCreateGroupOptions(
                    permissions = permissions,
                    groupName = groupName,
                    groupImageUrlSquare = groupImageUrlSquare,
                    groupDescription = groupDescription,
                    customPermissionPolicySet = permissionsPolicySet,
                    messageDisappearingSettings = messageDisappearingSettings
                )
            )
        return Group(client, group)
    }

    suspend fun newGroup(
        inboxIds: List<InboxId>,
        permissions: GroupPermissionPreconfiguration = GroupPermissionPreconfiguration.ALL_MEMBERS,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Group {
        return newGroupInternal(
            inboxIds,
            GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(permissions),
            groupName,
            groupImageUrlSquare,
            groupDescription,
            null,
            disappearingMessageSettings?.let {
                FfiMessageDisappearingSettings(
                    it.disappearStartingAtNs,
                    it.retentionDurationInNs
                )
            }
        )
    }

    suspend fun newGroupCustomPermissions(
        inboxIds: List<InboxId>,
        permissionPolicySet: PermissionPolicySet,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Group {
        return newGroupInternal(
            inboxIds,
            FfiGroupPermissionsOptions.CUSTOM_POLICY,
            groupName,
            groupImageUrlSquare,
            groupDescription,
            PermissionPolicySet.toFfiPermissionPolicySet(permissionPolicySet),
            disappearingMessageSettings?.let {
                FfiMessageDisappearingSettings(
                    it.disappearStartingAtNs,
                    it.retentionDurationInNs
                )
            }
        )
    }

    private suspend fun newGroupInternal(
        inboxIds: List<InboxId>,
        permissions: FfiGroupPermissionsOptions,
        groupName: String,
        groupImageUrlSquare: String,
        groupDescription: String,
        permissionsPolicySet: FfiPermissionPolicySet?,
        messageDisappearingSettings: FfiMessageDisappearingSettings?,
    ): Group {
        validateInboxIds(inboxIds)
        val group =
            ffiConversations.createGroupWithInboxIds(
                inboxIds,
                opts = FfiCreateGroupOptions(
                    permissions = permissions,
                    groupName = groupName,
                    groupImageUrlSquare = groupImageUrlSquare,
                    groupDescription = groupDescription,
                    customPermissionPolicySet = permissionsPolicySet,
                    messageDisappearingSettings = messageDisappearingSettings
                )
            )
        return Group(client, group)
    }

    fun newGroupOptimistic(
        permissions: GroupPermissionPreconfiguration = GroupPermissionPreconfiguration.ALL_MEMBERS,
        groupName: String = "",
        groupImageUrlSquare: String = "",
        groupDescription: String = "",
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Group {
        val group = ffiConversations.createGroupOptimistic(
            opts = FfiCreateGroupOptions(
                permissions = GroupPermissionPreconfiguration.toFfiGroupPermissionOptions(
                    permissions
                ),
                groupName = groupName,
                groupImageUrlSquare = groupImageUrlSquare,
                groupDescription = groupDescription,
                customPermissionPolicySet = null,
                messageDisappearingSettings = disappearingMessageSettings?.let {
                    FfiMessageDisappearingSettings(
                        it.disappearStartingAtNs,
                        it.retentionDurationInNs
                    )
                }
            )
        )
        return Group(client, group)
    }

    // Sync from the network the latest list of conversations
    suspend fun sync() {
        ffiConversations.sync()
    }

    // Sync all new and existing conversations data from the network
    suspend fun syncAllConversations(consentStates: List<ConsentState>? = null): UInt {
        return ffiConversations.syncAllConversations(
            consentStates?.let { states ->
                states.map { ConsentState.toFfiConsentState(it) }
            }
        )
    }

    suspend fun newConversationWithIdentity(
        peerPublicIdentity: PublicIdentity,
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Conversation {
        val dm = findOrCreateDmWithIdentity(peerPublicIdentity, disappearingMessageSettings)
        return Conversation.Dm(dm)
    }

    suspend fun findOrCreateDmWithIdentity(
        peerPublicIdentity: PublicIdentity,
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Dm {
        if (peerPublicIdentity.identifier in client.inboxState(false).identities.map { it.identifier }) {
            throw XMTPException("Recipient is sender")
        }

        val dmConversation = ffiConversations.findOrCreateDm(
            peerPublicIdentity.ffiPrivate,
            opts = FfiCreateDmOptions(
                disappearingMessageSettings?.let {
                    FfiMessageDisappearingSettings(
                        it.disappearStartingAtNs,
                        it.retentionDurationInNs
                    )
                }
            )
        )
        return Dm(client, dmConversation)
    }

    suspend fun newConversation(
        peerInboxId: InboxId,
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Conversation {
        val dm = findOrCreateDm(peerInboxId, disappearingMessageSettings)
        return Conversation.Dm(dm)
    }

    suspend fun findOrCreateDm(
        peerInboxId: InboxId,
        disappearingMessageSettings: DisappearingMessageSettings? = null,
    ): Dm {
        validateInboxId(peerInboxId)
        if (peerInboxId == client.inboxId) {
            throw XMTPException("Recipient is sender")
        }
        val dmConversation = ffiConversations.findOrCreateDmByInboxId(
            peerInboxId,
            opts = FfiCreateDmOptions(
                disappearingMessageSettings?.let {
                    FfiMessageDisappearingSettings(
                        it.disappearStartingAtNs,
                        it.retentionDurationInNs
                    )
                }
            )
        )
        return Dm(client, dmConversation)
    }

    fun listGroups(
        createdAfterNs: Long? = null,
        createdBeforeNs: Long? = null,
        lastActivityAfterNs: Long? = null,
        lastActivityBeforeNs: Long? = null,
        limit: Int? = null,
        consentStates: List<ConsentState>? = null,
        orderBy: ListConversationsOrderBy = ListConversationsOrderBy.LAST_ACTIVITY,
    ): List<Group> {
        val ffiGroups = ffiConversations.listGroups(
            opts = FfiListConversationsOptions(
                createdAfterNs,
                createdBeforeNs,
                lastActivityBeforeNs,
                lastActivityAfterNs,
                limit = limit?.toLong(),
                consentStates = consentStates?.let { states ->
                    states.map { ConsentState.toFfiConsentState(it) }
                },
                orderBy = orderBy.toFfi(),
                includeDuplicateDms = false
            )
        )

        return ffiGroups.map {
            Group(client, it.conversation(), it.lastMessage())
        }
    }

    fun listDms(
        createdAfterNs: Long? = null,
        createdBeforeNs: Long? = null,
        lastActivityAfterNs: Long? = null,
        lastActivityBeforeNs: Long? = null,
        limit: Int? = null,
        consentStates: List<ConsentState>? = null,
        orderBy: ListConversationsOrderBy = ListConversationsOrderBy.LAST_ACTIVITY,
    ): List<Dm> {
        val ffiDms = ffiConversations.listDms(
            opts = FfiListConversationsOptions(
                createdAfterNs,
                createdBeforeNs,
                lastActivityBeforeNs,
                lastActivityAfterNs,
                limit = limit?.toLong(),
                consentStates = consentStates?.let { states ->
                    states.map { ConsentState.toFfiConsentState(it) }
                },
                orderBy = orderBy.toFfi(),
                includeDuplicateDms = false
            )
        )

        return ffiDms.map {
            Dm(client, it.conversation(), it.lastMessage())
        }
    }

    suspend fun list(
        createdAfterNs: Long? = null,
        createdBeforeNs: Long? = null,
        lastActivityAfterNs: Long? = null,
        lastActivityBeforeNs: Long? = null,
        limit: Int? = null,
        consentStates: List<ConsentState>? = null,
        orderBy: ListConversationsOrderBy = ListConversationsOrderBy.LAST_ACTIVITY,
    ): List<Conversation> {
        val ffiConversation = ffiConversations.list(
            opts = FfiListConversationsOptions(
                createdAfterNs = createdAfterNs,
                createdBeforeNs = createdBeforeNs,
                lastActivityBeforeNs = lastActivityBeforeNs,
                lastActivityAfterNs = lastActivityAfterNs,
                limit = limit?.toLong(),
                consentStates = consentStates?.let { states ->
                    states.map { ConsentState.toFfiConsentState(it) }
                },
                orderBy = orderBy.toFfi(),
                includeDuplicateDms = false
            )
        )

        return ffiConversation.map { it.toConversation() }
    }

    private suspend fun FfiConversationListItem.toConversation(): Conversation {
        return when (conversation().conversationType()) {
            FfiConversationType.DM -> Conversation.Dm(
                Dm(
                    client,
                    conversation(),
                    lastMessage(),
                    isCommitLogForked()
                )
            )

            else -> Conversation.Group(
                Group(
                    client,
                    conversation(),
                    lastMessage(),
                    isCommitLogForked()
                )
            )
        }
    }

    fun stream(
        type: ConversationFilterType = ConversationFilterType.ALL,
        onClose: (() -> Unit)? = null,
    ): Flow<Conversation> =
        callbackFlow {
            val conversationCallback = object : FfiConversationCallback {
                override fun onConversation(conversation: FfiConversation) {
                    launch(Dispatchers.IO) {
                        when (conversation.conversationType()) {
                            FfiConversationType.DM -> trySend(
                                Conversation.Dm(
                                    Dm(
                                        client,
                                        conversation
                                    )
                                )
                            )

                            else -> trySend(Conversation.Group(Group(client, conversation)))
                        }
                    }
                }

                override fun onError(error: FfiSubscribeException) {
                    Log.e("XMTP Conversation stream", error.message.toString())
                }

                override fun onClose() {
                    onClose?.invoke()
                    close()
                }
            }

            val stream = when (type) {
                ConversationFilterType.ALL -> ffiConversations.stream(conversationCallback)
                ConversationFilterType.GROUPS -> ffiConversations.streamGroups(conversationCallback)
                ConversationFilterType.DMS -> ffiConversations.streamDms(conversationCallback)
            }

            awaitClose { stream.end() }
        }

    fun streamAllMessages(
        type: ConversationFilterType = ConversationFilterType.ALL,
        consentStates: List<ConsentState>? = null,
        onClose: (() -> Unit)? = null,
    ): Flow<DecodedMessage> =
        callbackFlow {
            val messageCallback = object : FfiMessageCallback {
                override fun onMessage(message: FfiMessage) {
                    val decodedMessage = DecodedMessage.create(message)
                    decodedMessage?.let { trySend(it) }
                }

                override fun onError(error: FfiSubscribeException) {
                    Log.e("XMTP all message stream", error.message.toString())
                }

                override fun onClose() {
                    onClose?.invoke()
                    close()
                }
            }
            val states = consentStates?.let { states ->
                states.map { ConsentState.toFfiConsentState(it) }
            }

            val stream = when (type) {
                ConversationFilterType.ALL -> ffiConversations.streamAllMessages(
                    messageCallback,
                    states
                )

                ConversationFilterType.GROUPS -> ffiConversations.streamAllGroupMessages(
                    messageCallback,
                    states
                )

                ConversationFilterType.DMS -> ffiConversations.streamAllDmMessages(
                    messageCallback,
                    states
                )
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

    fun allPushTopics(): List<String> {
        val conversations = ffiConversations.list(
            FfiListConversationsOptions(
                null,
                null,
                null,
                null,
                ListConversationsOrderBy.CREATED_AT.toFfi(),
                null,
                null,
                includeDuplicateDms = true
            )
        )
        return conversations.map { Topic.groupMessage(it.conversation().id().toHex()).description }
    }
}

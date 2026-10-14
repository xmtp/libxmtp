package org.xmtp.android.example.messenger

import kotlinx.coroutines.CancellationException
import uniffi.xmtp_sdk.Conversation
import uniffi.xmtp_sdk.Member
import uniffi.xmtp_sdk.members

/** Use the current verified peer handle for a DM and the chosen name for a group. */
internal suspend fun Conversation.displayTitle(
    groupName: String? = null,
    knownMembers: List<Member>? = null,
): String =
    when (this) {
        is Conversation.Group -> {
            (groupName ?: group.state().name).ifBlank { "Group" }
        }

        is Conversation.Dm -> {
            val peer = dm.peerInboxId()
            val handle =
                try {
                    (knownMembers ?: members()).firstOrNull { it.inboxId == peer }?.handle
                } catch (error: Throwable) {
                    if (error is CancellationException) throw error
                    null
                }
            handle?.let { "@$it" } ?: peer?.take(12) ?: "Direct message"
        }
    }

package org.xmtp.android.example.extension

import kotlinx.coroutines.flow.Flow
import uniffi.xmtp_sdk.*

fun Message.displayBody(): String =
    when (val body = content) {
        is SDKMessageContent.Standard -> {
            when (val value = body.value) {
                is MessageContent.Text -> value.v1
                is MessageContent.GroupUpdated -> "Group updated"
                else -> fallback.orEmpty()
            }
        }

        is SDKMessageContent.Custom -> {
            body.value?.toString() ?: fallback.orEmpty()
        }

        is SDKMessageContent.Unknown -> {
            fallback ?: "Unknown content"
        }
    }

fun SDKClient.messageStream(conversation: Conversation): Flow<Message> = conversation.streamMessages()

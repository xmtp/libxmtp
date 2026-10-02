package org.xmtp.android.example.conversation

import uniffi.xmtp_sdk.*

interface ConversationsClickListener {
    fun onConversationClick(conversation: Conversation)

    fun onFooterClick(address: String)
}

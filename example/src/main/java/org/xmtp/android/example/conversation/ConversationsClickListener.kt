package org.xmtp.android.example.conversation

import org.xmtp.android.library.Conversation

interface ConversationsClickListener {
    fun onConversationClick(conversation: Conversation)
    fun onFooterClick(address: String)
}

package org.xmtp.android.example.conversation

import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.MainViewModel
import org.xmtp.android.example.R
import org.xmtp.android.example.databinding.ListItemConversationBinding
import org.xmtp.android.example.extension.truncatedAddress
import org.xmtp.android.library.Conversation
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChanges

class ConversationViewHolder(
    private val binding: ListItemConversationBinding,
    clickListener: ConversationsClickListener,
) : RecyclerView.ViewHolder(binding.root) {

    private var conversation: Conversation? = null

    init {
        binding.root.setOnClickListener {
            conversation?.let {
                clickListener.onConversationClick(it)
            }
        }
    }

    fun bind(item: MainViewModel.MainListItem.ConversationItem) {
        conversation = item.conversation
        binding.peerAddress.text = if (item.conversation.peerAddress.contains(",")) {
            val addresses = item.conversation.peerAddress.split(",")
            addresses.joinToString(" & ") {
                it.truncatedAddress()
            }
        } else {
            item.conversation.peerAddress.truncatedAddress()
        }

        val messageBody: String = if (item.mostRecentMessage?.content<Any>() is String) {
            item.mostRecentMessage.body.orEmpty()
        } else if (item.mostRecentMessage?.content<Any>() is GroupMembershipChanges) {
            val changes = item.mostRecentMessage.content() as? GroupMembershipChanges
            "Membership Changed ${
                changes?.membersAddedList?.mapNotNull { it.accountAddress }.toString()
            }"
        } else {
            ""
        }
        val isMe = item.mostRecentMessage?.senderAddress == ClientManager.client.address
        if (messageBody.isNotBlank()) {
            binding.messageBody.text = if (isMe) binding.root.resources.getString(
                R.string.your_message_body,
                messageBody
            ) else messageBody
        } else {
            binding.messageBody.text = binding.root.resources.getString(R.string.empty_message)
        }
    }
}

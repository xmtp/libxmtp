package org.xmtp.android.example.conversation

import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.MainViewModel
import org.xmtp.android.example.databinding.ListItemConversationBinding
import org.xmtp.android.library.Conversation

class ConversationViewHolder(
    private val binding: ListItemConversationBinding,
    clickListener: ConversationsClickListener
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
        binding.peerAddress.text = item.conversation.peerAddress
    }
}

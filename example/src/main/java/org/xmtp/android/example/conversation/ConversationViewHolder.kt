package org.xmtp.android.example.conversation

import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.MainViewModel
import org.xmtp.android.example.databinding.ListItemConversationBinding

class ConversationViewHolder(private val binding: ListItemConversationBinding) :
    RecyclerView.ViewHolder(binding.root) {

    fun bind(item: MainViewModel.MainListItem.Conversation) {
        binding.peerAddress.text = item.peerAddress
    }
}

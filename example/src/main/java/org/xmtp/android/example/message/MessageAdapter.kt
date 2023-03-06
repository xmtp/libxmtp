package org.xmtp.android.example.message

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.conversation.ConversationDetailViewModel
import org.xmtp.android.example.databinding.ListItemMessageBinding

class MessageAdapter : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    private val listItems = mutableListOf<ConversationDetailViewModel.MessageListItem>()

    fun setData(newItems: List<ConversationDetailViewModel.MessageListItem>) {
        listItems.clear()
        listItems.addAll(newItems)
        notifyItemRangeChanged(0, newItems.size)
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return when (viewType) {
            ConversationDetailViewModel.MessageListItem.ITEM_TYPE_MESSAGE -> {
                val binding = ListItemMessageBinding.inflate(inflater, parent, false)
                MessageViewHolder(binding)
            }
            else -> throw IllegalArgumentException("Unsupported view type $viewType")
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        val item = listItems[position]
        when (holder) {
            is MessageViewHolder -> {
                holder.bind(item as ConversationDetailViewModel.MessageListItem.Message)
            }
            else -> throw IllegalArgumentException("Unsupported view holder")
        }
    }

    override fun getItemViewType(position: Int) = listItems[position].itemType

    override fun getItemCount() = listItems.count()

    override fun getItemId(position: Int) = listItems[position].id.hashCode().toLong()
}

package org.xmtp.android.example.conversation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.MainViewModel
import org.xmtp.android.example.databinding.ListItemConversationBinding
import org.xmtp.android.example.databinding.ListItemConversationFooterBinding

class ConversationsAdapter(
    private val clickListener: ConversationsClickListener
) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {

    init {
        setHasStableIds(true)
    }

    private val listItems = mutableListOf<MainViewModel.MainListItem>()

    fun setData(newItems: List<MainViewModel.MainListItem>) {
        listItems.clear()
        listItems.addAll(newItems)
        notifyDataSetChanged()
    }

    fun addItem(item: MainViewModel.MainListItem) {
        listItems.add(0, item)
        notifyDataSetChanged()
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val inflater = LayoutInflater.from(parent.context)
        return when (viewType) {
            MainViewModel.MainListItem.ITEM_TYPE_CONVERSATION -> {
                val binding = ListItemConversationBinding.inflate(inflater, parent, false)
                ConversationViewHolder(binding, clickListener)
            }
            MainViewModel.MainListItem.ITEM_TYPE_FOOTER -> {
                val binding = ListItemConversationFooterBinding.inflate(inflater, parent, false)
                ConversationFooterViewHolder(binding, clickListener)
            }
            else -> throw IllegalArgumentException("Unsupported view type $viewType")
        }
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        val item = listItems[position]
        when (holder) {
            is ConversationViewHolder -> {
                holder.bind(item as MainViewModel.MainListItem.ConversationItem)
            }
            is ConversationFooterViewHolder -> {
                holder.bind(item as MainViewModel.MainListItem.Footer)
            }
            else -> throw IllegalArgumentException("Unsupported view holder")
        }
    }

    override fun getItemViewType(position: Int) = listItems[position].itemType

    override fun getItemCount() = listItems.count()

    override fun getItemId(position: Int) = listItems[position].id.hashCode().toLong()
}

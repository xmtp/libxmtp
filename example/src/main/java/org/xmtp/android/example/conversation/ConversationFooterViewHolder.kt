package org.xmtp.android.example.conversation

import android.graphics.Color
import android.graphics.Typeface
import android.text.Spannable
import android.text.SpannableString
import android.text.style.ForegroundColorSpan
import android.text.style.StyleSpan
import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.MainViewModel
import org.xmtp.android.example.R
import org.xmtp.android.example.databinding.ListItemConversationFooterBinding

class ConversationFooterViewHolder(
    private val binding: ListItemConversationFooterBinding,
    onFooterClickListener: ConversationsClickListener
) : RecyclerView.ViewHolder(binding.root) {

    private var address: String? = null

    init {
        binding.root.setOnClickListener {
            address?.let {
                onFooterClickListener.onFooterClick(it)
            }
        }
    }

    fun bind(item: MainViewModel.MainListItem.Footer) {
        address = item.address
        val spannable = SpannableString(
            binding.root.resources.getString(
                R.string.conversation_footer,
                item.address,
                item.environment
            )
        )
        val addressStart = spannable.indexOf(item.address)
        val envStart = spannable.indexOf(item.environment)
        spannable.setSpan(
            StyleSpan(Typeface.BOLD),
            addressStart,
            addressStart + item.address.length,
            Spannable.SPAN_INCLUSIVE_EXCLUSIVE
        )
        spannable.setSpan(
            ForegroundColorSpan(Color.BLACK),
            addressStart,
            addressStart + item.address.length,
            Spannable.SPAN_INCLUSIVE_EXCLUSIVE
        )
        spannable.setSpan(
            StyleSpan(Typeface.BOLD),
            envStart,
            envStart + item.environment.length,
            Spannable.SPAN_INCLUSIVE_EXCLUSIVE
        )
        spannable.setSpan(
            ForegroundColorSpan(Color.BLACK),
            envStart,
            envStart + item.environment.length,
            Spannable.SPAN_INCLUSIVE_EXCLUSIVE
        )
        binding.footer.text = spannable
    }
}

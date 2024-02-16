package org.xmtp.android.example.message

import android.annotation.SuppressLint
import android.graphics.Color
import androidx.constraintlayout.widget.ConstraintLayout
import androidx.constraintlayout.widget.ConstraintLayout.LayoutParams.PARENT_ID
import androidx.constraintlayout.widget.ConstraintLayout.LayoutParams.UNSET
import androidx.recyclerview.widget.RecyclerView
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.R
import org.xmtp.android.example.conversation.ConversationDetailViewModel
import org.xmtp.android.example.databinding.ListItemMessageBinding
import org.xmtp.android.example.extension.margins
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChanges
import java.text.SimpleDateFormat
import java.util.Locale

class MessageViewHolder(
    private val binding: ListItemMessageBinding,
) : RecyclerView.ViewHolder(binding.root) {

    private val marginLarge = binding.root.resources.getDimensionPixelSize(R.dimen.message_margin)
    private val marginSmall = binding.root.resources.getDimensionPixelSize(R.dimen.padding)
    private val backgroundMe = Color.LTGRAY
    private val backgroundPeer =
        binding.root.resources.getColor(R.color.teal_700, binding.root.context.theme)

    @SuppressLint("SetTextI18n")
    fun bind(item: ConversationDetailViewModel.MessageListItem.Message) {
        val isFromMe =
            ClientManager.client.address.lowercase() == item.message.senderAddress.lowercase()
        val params = binding.messageContainer.layoutParams as ConstraintLayout.LayoutParams
        if (isFromMe) {
            params.rightToRight = PARENT_ID
            params.leftToLeft = UNSET
            binding.messageRow.margins(left = marginLarge, right = marginSmall)
            binding.messageContainer.setCardBackgroundColor(backgroundMe)
            binding.messageBody.setTextColor(Color.BLACK)
        } else {
            params.leftToLeft = PARENT_ID
            params.rightToRight = UNSET
            binding.messageRow.margins(right = marginLarge, left = marginSmall)
            binding.messageContainer.setCardBackgroundColor(backgroundPeer)
            binding.messageBody.setTextColor(Color.WHITE)
        }
        binding.messageContainer.layoutParams = params
        if (item.message.content<Any>() is String) {
            binding.messageBody.text = item.message.body
            val sdf = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.getDefault())
            binding.messageDate.text = sdf.format(item.message.sent)

        } else if (item.message.content<Any>() is GroupMembershipChanges) {
            val changes = item.message.content() as? GroupMembershipChanges
            binding.messageBody.text =
                "Membership Changed ${
                    changes?.membersAddedList?.mapNotNull { it.accountAddress }.toString()
                }"
        }
    }
}

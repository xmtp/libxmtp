package org.xmtp.android.example.messenger

import java.time.ZoneId
import java.time.format.DateTimeFormatter
import org.xmtp.android.example.shared.*
import uniffi.xmtp_sdk.*

val visibleContentTypes = listOf("text", "reply", "remoteStaticAttachment").map { ContentTypeId("xmtp.org", it, 1u, 0u) }
fun publishedSelection() = ListMessagesOptions(deliveryStatus = DeliveryStatus.PUBLISHED, kind = MessageKind.APPLICATION, contentTypes = visibleContentTypes, direction = MessageOrder.DESCENDING, sortBy = MessageSortBy.SENT_AT)
fun incomingSelection(own: InboxId, marker: Long? = null) = publishedSelection().copy(excludeSenderInboxIds = listOf(own), insertedAfter = marker?.let(::Timestamp))
fun commonState(conversation: Conversation, group: GroupState?, dm: ConversationState?) = group?.common ?: checkNotNull(dm)
suspend fun conversationState(conversation: Conversation): ConversationState = when (conversation) { is Conversation.Group -> conversation.group.state().common; is Conversation.Dm -> conversation.dm.state() }
suspend fun logicalConversationKey(conversation: Conversation, own: InboxId): String = when (conversation) {
    is Conversation.Group -> conversation.id()
    is Conversation.Dm -> "dm:" + (conversation.dm.peerInboxIds() + own).distinct().sorted().joinToString(":")
}
private fun messageText(content: MessageContent): String = when (content) {
    is MessageContent.Text -> content.v1
    is MessageContent.Markdown -> content.v1
    is MessageContent.Reply -> when (val body = content.body) { is MessageBody.Text -> body.v1; is MessageBody.Markdown -> body.v1; is MessageBody.DeletedMessage -> "Message deleted"; else -> "Unsupported reply" }
    is MessageContent.RemoteAttachment -> content.v1.filename ?: "File"
    is MessageContent.DeletedMessage -> "Message deleted"
    else -> "Unsupported content"
}
fun Message.toRow(own: InboxId): MessageRow {
    val standard = (content as? SDKMessageContent.Standard)?.value
    val time = sentAt.date.atZone(ZoneId.systemDefault())
    val deleted = standard is MessageContent.DeletedMessage
    val reactions = reactions.groupBy { it.reaction.content }.map { (emoji, values) -> ReactionUi(emoji, values.size, values.any { it.senderInboxId == own }) }
    return MessageRow(id, if (senderInboxId == own) "You" else senderInboxId.take(8), standard?.let(::messageText) ?: fallback ?: "Unsupported content", time.format(DateTimeFormatter.ofPattern("HH:mm")), time.format(DateTimeFormatter.ofPattern("MMM d")), sentAt.ns, senderInboxId == own, when (deliveryStatus) { DeliveryStatus.PUBLISHED -> "Delivered"; DeliveryStatus.FAILED -> "Failed"; else -> "Queued" }, inReplyTo?.let { parent -> when (val body = parent.content) { is MessageBody.Text -> body.v1; is MessageBody.DeletedMessage -> "Message deleted"; else -> parent.fallback ?: "Reply" } }, reactions, deleted, standard is MessageContent.RemoteAttachment)
}

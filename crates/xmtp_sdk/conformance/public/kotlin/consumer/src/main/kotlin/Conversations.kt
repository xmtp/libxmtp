import uniffi.xmtp_sdk.*

// Each union parameter has same-name overloads. Empty lists need an explicit
// element type.
suspend fun consumeIdentityRoutes(
    client: SDKClient,
    identity: PublicIdentity,
) {
    val conversations = client.conversations()
    val byInbox: Group = conversations.createGroup(listOf(client.inboxId()))
    val byIdentity: Group = conversations.createGroup(listOf(identity), null)
    conversations.createGroup(emptyList<InboxId>())
    conversations.createGroup(emptyList<PublicIdentity>())
    val inboxDm: Dm = conversations.createDm(client.inboxId())
    val dm: Dm = conversations.createDm(identity, null)
    val addedByInbox: MembershipResult = byInbox.addMembers(listOf(client.inboxId()))
    val addedByIdentity: MembershipResult = byIdentity.addMembers(listOf(identity))
    byInbox.removeMembers(listOf(client.inboxId()))
    byIdentity.removeMembers(listOf(identity))
    val peer: InboxId? = dm.peerInboxId()
    val creator: String? = byInbox.creatorInboxId()
    val adder: String? = dm.addedByInboxId()
    val isCreator: Boolean = byInbox.isCreator()
    println(listOf(inboxDm, addedByInbox, addedByIdentity, peer, creator, adder, isCreator))
}

suspend fun consumeMessageActions(
    message: Message,
    reaction: Reaction,
) {
    val owner: SDKClient = message.client()
    val refreshed: Message? = message.refresh()
    val reacted: MessageId = message.react(reaction)
    val replied: MessageId = message.reply("reply")
    val parent: Message? = message.parent()
    val conversation: Conversation? = message.conversation()
    val cursor: String? = message.deliveryCursor
    val deleted: MessageId = message.delete()
    message.deleteLocally()
    println(listOf(owner, refreshed, reacted, replied, parent, conversation, cursor, deleted))
}

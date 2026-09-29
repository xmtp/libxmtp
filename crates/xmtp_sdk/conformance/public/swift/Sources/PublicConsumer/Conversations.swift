import XmtpSdk

// Each union parameter has same-name overloads. Empty lists need an explicit
// element type because both overloads accept an empty literal.
public func consumeIdentityRoutes(_ client: SDKClient, _ identity: PublicIdentity) async throws {
    let conversations = client.conversations()
    let byInbox: Group = try await conversations.createGroup(members: [client.inboxId()])
    let byIdentity: Group = try await conversations.createGroup(members: [identity], options: nil)
    _ = try await conversations.createGroup(members: [InboxId]())
    _ = try await conversations.createGroup(members: [PublicIdentity]())
    let _: Dm = try await conversations.createDm(peer: client.inboxId())
    let dm: Dm = try await conversations.createDm(peer: identity, options: nil)
    let _: MembershipResult = try await byInbox.addMembers(members: [client.inboxId()])
    let _: MembershipResult = try await byIdentity.addMembers(members: [identity])
    try await byInbox.removeMembers(members: [client.inboxId()])
    try await byIdentity.removeMembers(members: [identity])
    let _: InboxId? = try await dm.peerInboxId()
    let creator: String? = byInbox.creatorInboxId()
    let adder: String? = dm.addedByInboxId()
    let _: Bool = byInbox.isCreator()
    _ = (creator, adder)
}

public func consumeMessageActions(_ message: Message, _ reaction: Reaction) async throws {
    let _: SDKClient = try message.client()
    let _: Message? = try await message.refresh()
    let _: MessageId = try await message.react(reaction)
    let _: MessageId = try await message.reply("reply")
    let _: Message? = try await message.parent()
    let _: Conversation? = try await message.conversation()
    let _: String? = message.deliveryCursor
    let _: MessageId = try await message.delete()
    try await message.deleteLocally()
}

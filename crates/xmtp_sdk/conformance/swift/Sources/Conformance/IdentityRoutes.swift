import XmtpSdk

private func expectMembers(_ group: Group, _ expected: Set<InboxId>) async throws {
    let members = try Set(await group.members().map(\.inboxId))
    guard members == expected else { throw ConformanceFailure("members \(members) != \(expected)") }
}

/// Each account-identity overload performs the membership change of its inbox
/// ID form. The host Client forwards identity without the generated Client.
func checkIdentityRoutes(backend: BackendOptions) async throws {
    let options = ClientOptions(backend: .options(options: backend), storage: StorageOptions(location: .inMemory), deviceSync: false)
    let a = try await SDKClient.create(signer: await generateLocalSigner(), options: options)
    let b = try await SDKClient.create(signer: await generateLocalSigner(), options: options)
    let conversations = a.conversations()
    let empty = try await conversations.createGroup(members: [PublicIdentity]())
    try await expectMembers(empty, [a.inboxId()])
    let creator: String? = empty.creatorInboxId()
    let adder: String? = empty.addedByInboxId()
    precondition(creator == a.inboxId() && adder == a.inboxId() && empty.isCreator())
    let group = try await conversations.createGroup(members: [b.identity()])
    try await expectMembers(group, [a.inboxId(), b.inboxId()])
    try await group.removeMembers(members: [b.identity()])
    try await expectMembers(group, [a.inboxId()])
    let added = try await group.addMembers(members: [b.identity()])
    precondition(added.added == [b.inboxId()])
    try await expectMembers(group, [a.inboxId(), b.inboxId()])
    let dm = try await conversations.createDm(peer: b.identity())
    let peer: InboxId? = try await dm.peerInboxId()
    precondition(peer == b.inboxId())
    let byInbox = try await conversations.createDm(peer: b.inboxId())
    precondition(byInbox.id() == dm.id())
    try await b.end()
    try await a.end()
    print("Swift identity routes and optional received identity passed")
}

/// Account-identity forms of the membership methods use the same names as
/// their inbox ID forms. An empty literal list needs an explicit element type.
public extension Conversations {
    func createGroup(members: [PublicIdentity], options: CreateGroupOptions? = nil) async throws -> Group {
        try await createGroupWithIdentities(members: members, options: options)
    }

    func createDm(peer: PublicIdentity, options: CreateDmOptions? = nil) async throws -> Dm {
        try await createDmWithIdentity(peer: peer, options: options)
    }
}

public extension Group {
    func addMembers(members: [PublicIdentity]) async throws -> MembershipResult {
        try await addMembersByIdentity(members: members)
    }

    func removeMembers(members: [PublicIdentity]) async throws {
        try await removeMembersByIdentity(members: members)
    }
}

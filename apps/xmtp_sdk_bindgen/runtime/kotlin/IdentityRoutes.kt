package uniffi.xmtp_sdk

// Account-identity forms of the membership methods use the same names as their
// inbox ID forms. An empty list needs an explicit element type.

suspend fun Conversations.createGroup(
    members: List<PublicIdentity>,
    options: CreateGroupOptions? = null,
): Group = createGroupWithIdentities(members, options)

suspend fun Conversations.createDm(
    peer: PublicIdentity,
    options: CreateDmOptions? = null,
): Dm = createDmWithIdentity(peer, options)

suspend fun Group.addMembers(members: List<PublicIdentity>): MembershipResult = addMembersByIdentity(members)

suspend fun Group.removeMembers(members: List<PublicIdentity>) = removeMembersByIdentity(members)

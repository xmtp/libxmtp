import uniffi.xmtp_sdk.*

private suspend fun memberIds(group: Group): Set<InboxId> = group.members().map { it.inboxId }.toSet()

/**
 * Each account-identity overload performs the membership change of its inbox
 * ID form. The host Client forwards identity without the generated Client.
 */
suspend fun checkIdentityRoutes(backend: BackendOptions) {
    val options =
        ClientOptions(
            backend = BackendSource.Options(backend),
            storage = StorageOptions(location = StorageLocation.InMemory),
            deviceSync = false,
        )
    val a = SDKClient.create(generateLocalSigner(), options)
    val b = SDKClient.create(generateLocalSigner(), options)
    try {
        val conversations = a.conversations()
        val empty = conversations.createGroup(emptyList<PublicIdentity>())
        check(memberIds(empty) == setOf(a.inboxId()))
        val creator: String? = empty.creatorInboxId()
        val adder: String? = empty.addedByInboxId()
        check(creator == a.inboxId() && adder == a.inboxId() && empty.isCreator())
        val group = conversations.createGroup(listOf(b.identity()))
        check(memberIds(group) == setOf(a.inboxId(), b.inboxId()))
        group.removeMembers(listOf(b.identity()))
        check(memberIds(group) == setOf(a.inboxId()))
        val added = group.addMembers(listOf(b.identity()))
        check(added.added == listOf(b.inboxId()))
        check(memberIds(group) == setOf(a.inboxId(), b.inboxId()))
        val dm = conversations.createDm(b.identity())
        val peer: InboxId? = dm.peerInboxId()
        check(peer == b.inboxId())
        check(conversations.createDm(b.inboxId()).id() == dm.id())
    } finally {
        b.end()
        a.end()
    }
    println("Kotlin identity routes and optional received identity passed")
}

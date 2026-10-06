package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The PublicIdentity overloads in IdentityRoutes.kt are hand-written Kotlin. Each
// one must reach the identity method of the same name, not the inbox ID form.
// They call internal generated methods, so this test needs a native client.
// Rust tests the methods themselves:
// xmtp_sdk/src/tests/identity_routes.rs::identity_routes_change_membership_by_account.
class IdentityRoutesTest {
    private suspend fun memberIds(group: Group): Set<InboxId> = group.members().map { it.inboxId }.toSet()

    @Test
    fun identityOverloadsChangeMembership() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val a = create()
                    val b = create()
                    val conversations = a.conversations()
                    val empty = conversations.createGroup(emptyList<PublicIdentity>())
                    assertEquals(setOf(a.inboxId()), memberIds(empty))
                    assertEquals(a.inboxId(), empty.creatorInboxId())
                    assertEquals(a.inboxId(), empty.addedByInboxId())
                    assertTrue(empty.isCreator())

                    val group = conversations.createGroup(listOf(b.identity()))
                    assertEquals(setOf(a.inboxId(), b.inboxId()), memberIds(group))
                    group.removeMembers(listOf(b.identity()))
                    assertEquals(setOf(a.inboxId()), memberIds(group))
                    val added = group.addMembers(listOf(b.identity()))
                    assertEquals(listOf(b.inboxId()), added.added)
                    assertEquals(setOf(a.inboxId(), b.inboxId()), memberIds(group))

                    val dm = conversations.createDm(b.identity())
                    assertEquals(b.inboxId(), dm.peerInboxId())
                    assertEquals(dm.id(), conversations.createDm(b.inboxId()).id())
                }
            }
        }
}

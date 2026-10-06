package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// Generated calls whose only Android check was the Kotlin conformance program:
// archive bytes, decodeContent, prepare and publish, the Conversation
// lastMessage forward, duplicate DMs and the debug record's epoch. Each value
// crosses the native boundary once. Rust owns the behavior: xmtp_sdk/src/tests/archives.rs::consent_archive_storage_and_diagnostics,
// client_setup.rs::standard_content_decodes_text,
// reader_cursor.rs::delivery_cursor_absent_until_publication,
// reader_restored.rs::foreign_restored_dm_has_no_local_peer.
class GeneratedForwardsTest {
    @Test
    fun generatedCallsCrossTheNativeBoundary() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val alix = create()
                    val bo = create()
                    val group = alix.conversations.createGroup(listOf(bo.inboxId()))
                    // One commit moves the generated debug record's epoch by one.
                    val epoch = group.debugInfo().epoch
                    group.addAdmin(bo.inboxId())
                    assertEquals(epoch + 1uL, group.debugInfo().epoch)
                    val sentId = group.sendText("latest")
                    assertEquals(sentId, Conversation.Group(group).lastMessage()?.id)
                    assertEquals(MessageContent.Text("decoded"), alix.decodeContent(encodeText("decoded")))

                    val preparedId = group.prepareMessage(encodeText("prepared"))
                    assertNull(checkNotNull(alix.conversations.getMessageById(preparedId)).deliveryCursor)
                    Conversation.Group(group).publishMessage(preparedId)
                    assertNotNull(checkNotNull(alix.conversations.getMessageById(preparedId)).deliveryCursor)

                    // Each peer creates its own DM, so after a sync each one is the
                    // other's single duplicate.
                    val dm = alix.conversations.createDm(bo.inboxId())
                    val other = bo.conversations.createDm(alix.inboxId())
                    assertTrue(dm.id() != other.id())
                    alix.conversations.syncAll(null)
                    assertEquals(listOf(other.id()), dm.duplicateDms().map { it.id() })

                    val key = ByteArray(32) { 7 }
                    val archive = alix.archives().exportToBytes(key, null)
                    assertTrue(archive.isNotEmpty())
                    assertEquals(0.toUShort(), alix.archives().metadataFromBytes(archive, key).backupVersion)
                    val restored = create()
                    restored.archives().importFromBytes(archive, key)
                    assertNotNull("The imported archive lost the group", restored.conversations.getById(group.id()))
                }
            }
        }
}

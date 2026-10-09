package org.xmtp.android.example.messenger.metadata

import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*

/** Real commits through the app controller and the published operator catalogue. */
class MetadataEditorInstrumentedTest {
    @Test fun registeredFieldsRoundTripBetweenTwoClients() = runBlocking {
        withTimeout(120_000) {
            val url = checkNotNull(InstrumentationRegistry.getArguments().getString("metadataBackendUrl")) { "Run the owned metadata catalogue fixture. metadataBackendUrl is required." }
            val context = InstrumentationRegistry.getInstrumentation().targetContext
            fun options() = ClientOptions(backend = BackendSource.Options(BackendOptions(url = url)), storage = StorageOptions(location = StorageLocation.Directory(context.filesDir.resolve("metadata-test/${UUID.randomUUID()}").absolutePath)))
            val clients = mutableListOf<SDKClient>()
            try {
                val alix = SDKClient.create(context, generateLocalSigner(), options()).also(clients::add)
                val bo = SDKClient.create(context, generateLocalSigner(), options()).also(clients::add)
                val catalogue = alix.serverConfiguration().applicationComponents
                assertEquals((0xc001..0xc008).map { it.toUShort() }, catalogue.map { it.componentId })
                assertEquals(MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.BYTES), catalogue.single { it.componentId == 0xc003.toUShort() }.componentType)
                assertEquals(MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING), catalogue.single { it.componentId == 0xc006.toUShort() }.componentType)
                assertEquals(MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.BYTES), catalogue.single { it.componentId == 0xc007.toUShort() }.componentType)
                val group = alix.conversations.createGroup(listOf(bo.inboxId()))
                bo.conversations.sync()
                val peer = checkNotNull(bo.conversations.getById(group.id()))
                val mine = Conversation.Group(group)
                val editor = MetadataEditorController(mine, alix.inboxId(), { true }, catalogue)
                editor.refresh()
                suspend fun edit(change: MetadataEdit) { editor.edit(change); assertNull(editor.state.value.error); peer.sync() }
                fun id(value: Int) = FieldUiId(value.toUShort())
                fun ref(value: Int) = MetadataFieldRef(value.toUShort(), null)

                edit(MetadataEdit.Scalar(id(0xc001), ""))
                assertEquals(MetadataValue.Scalar(FieldValue.String("")), peer.metadataValue(ref(0xc001)))
                edit(MetadataEdit.Scalar(id(0xc001), null))
                assertNull(peer.metadataValue(ref(0xc001)))
                edit(MetadataEdit.Scalar(id(0xc002), "0080ff"))
                assertArrayEquals(byteArrayOf(0, -128, -1), ((peer.metadataValue(ref(0xc002)) as MetadataValue.Scalar).v1 as FieldValue.Bytes).v1)
                edit(MetadataEdit.Entry(id(0xc003), EntryAction.INSERT, "01", "00ff"))
                edit(MetadataEdit.Entry(id(0xc003), EntryAction.INSERT, "02", "7f"))
                edit(MetadataEdit.Entry(id(0xc003), EntryAction.UPDATE, "01", "80"))
                assertArrayEquals(byteArrayOf(127), (peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(2))) as FieldValue.Bytes).v1)
                assertArrayEquals(byteArrayOf(-128), (peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(1))) as FieldValue.Bytes).v1)
                edit(MetadataEdit.Entry(id(0xc004), EntryAction.INSERT, "00ff"))
                assertArrayEquals(byteArrayOf(0, -1), ((peer.metadataValue(ref(0xc004)) as MetadataValue.Set).v1.single() as FieldKey.Bytes).v1)
                edit(MetadataEdit.Entry(id(0xc004), EntryAction.DELETE, "00ff"))
                assertTrue((peer.metadataValue(ref(0xc004)) as MetadataValue.Set).v1.isEmpty())
                edit(MetadataEdit.Entry(id(0xc005), EntryAction.INSERT, bo.inboxId()))
                assertEquals(FieldKey.InboxId(bo.inboxId()), (peer.metadataValue(ref(0xc005)) as MetadataValue.Set).v1.single())
                edit(MetadataEdit.Entry(id(0xc005), EntryAction.DELETE, bo.inboxId()))

                val denied = MetadataEditorController(peer, bo.inboxId(), { true }, catalogue)
                denied.refresh()
                denied.edit(MetadataEdit.Scalar(id(0xc008), "denied"))
                assertNotNull(denied.failure)
                assertNotNull(denied.state.value.error)
                group.sync()
                assertNull(group.metadataValue(ref(0xc008)))

                suspend fun profiles(source: Conversation, other: Conversation) {
                    other.updateUserData(listOf(UserFieldUpdate(ref(0xc006), FieldValue.String("Bo"))))
                    source.sync()
                    val ownEditor = MetadataEditorController(source, alix.inboxId(), { true }, catalogue)
                    ownEditor.refresh()
                    ownEditor.edit(MetadataEdit.Own(mapOf(id(0xc006) to "Alix", id(0xc007) to "00ff")))
                    assertNull(ownEditor.state.value.error)
                    other.sync()
                    val readback = other.userData(listOf(ref(0xc006), ref(0xc007)), listOf(alix.inboxId(), bo.inboxId()))
                    val ownFields = readback.getValue(alix.inboxId()).associate { it.field.componentId to it.value }
                    assertEquals(FieldValue.String("Alix"), ownFields[0xc006.toUShort()])
                    assertArrayEquals(byteArrayOf(0, -1), (ownFields[0xc007.toUShort()] as FieldValue.Bytes).v1)
                    assertEquals(FieldValue.String("Bo"), readback.getValue(bo.inboxId()).single { it.field.componentId == 0xc006.toUShort() }.value)
                    ownEditor.edit(MetadataEdit.Own(mapOf(id(0xc006) to "", id(0xc007) to null)))
                    assertNull(ownEditor.state.value.error)
                    other.sync()
                    val cleared = other.userData(listOf(ref(0xc006), ref(0xc007)), listOf(alix.inboxId())).getValue(alix.inboxId())
                    assertEquals(FieldValue.String(""), cleared.single { it.field.componentId == 0xc006.toUShort() }.value)
                    assertFalse(cleared.any { it.field.componentId == 0xc007.toUShort() })
                    var current = true
                    val stale = MetadataEditorController(source, alix.inboxId(), { current }, catalogue)
                    stale.refresh()
                    current = false
                    stale.edit(MetadataEdit.Own(mapOf(id(0xc006) to "stale")))
                    assertEquals(FieldValue.String(""), source.userData(listOf(ref(0xc006)), listOf(alix.inboxId())).getValue(alix.inboxId()).single().value)
                }
                profiles(mine, peer)
                val dm = alix.conversations.createDm(bo.inboxId())
                bo.conversations.sync()
                profiles(Conversation.Dm(dm), checkNotNull(bo.conversations.getById(dm.id())))
                assertEquals(FieldValue.String(""), mine.userData(listOf(ref(0xc006)), listOf(alix.inboxId())).getValue(alix.inboxId()).single().value)
            } finally {
                withContext(NonCancellable) { clients.asReversed().forEach { it.end() } }
            }
        }
    }
}

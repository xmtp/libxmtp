package org.xmtp.android.example.messenger.metadata

import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.messenger.SessionFence
import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*
import java.util.UUID

/** Real commits through the app controller and the published operator catalogue. */
class MetadataEditorInstrumentedTest {
    private fun id(value: Int) = FieldUiId(value.toUShort())

    private fun ref(value: Int) = MetadataFieldRef(value.toUShort(), null)

    private suspend fun bytes(
        chat: Conversation,
        value: Int,
    ): ByteArray = ((chat.metadataValue(ref(value)) as MetadataValue.Scalar).v1 as FieldValue.Bytes).v1

    @Test fun registeredFieldsRoundTripBetweenTwoClients() =
        runBlocking {
            withTimeout(180_000) {
                val url =
                    checkNotNull(InstrumentationRegistry.getArguments().getString("metadataBackendUrl")) {
                        "Run the owned metadata catalogue fixture. metadataBackendUrl is required."
                    }
                val context = InstrumentationRegistry.getInstrumentation().targetContext

                val paths = mutableListOf<java.io.File>()

                fun options() =
                    ClientOptions(
                        backend = BackendSource.Options(BackendOptions(url = url)),
                        storage =
                            StorageOptions(
                                location =
                                    StorageLocation.Directory(
                                        context.filesDir
                                            .resolve(
                                                "metadata-test/${UUID.randomUUID()}",
                                            ).also(paths::add)
                                            .absolutePath,
                                    ),
                            ),
                    )
                val clients = mutableListOf<SDKClient>()
                val lifecycle = AndroidStreamLifecycle.enabled
                AndroidStreamLifecycle.enabled = false
                resumeStreams()
                try {
                    val alix = SDKClient.create(context, generateLocalSigner(), options()).also(clients::add)
                    val bo = SDKClient.create(context, generateLocalSigner(), options()).also(clients::add)
                    val catalogue = alix.serverConfiguration().applicationComponents
                    assertEquals((0xc001..0xc008).map { it.toUShort() }, catalogue.map { it.componentId })
                    assertEquals(
                        MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.BYTES),
                        catalogue.single { it.componentId == 0xc003.toUShort() }.componentType,
                    )
                    assertEquals(
                        MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING),
                        catalogue.single { it.componentId == 0xc006.toUShort() }.componentType,
                    )
                    assertEquals(
                        MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.BYTES),
                        catalogue.single { it.componentId == 0xc007.toUShort() }.componentType,
                    )
                    val group = alix.conversations.createGroup(listOf(bo.inboxId()))
                    bo.conversations.sync()
                    val mine = Conversation.Group(group)
                    val peer = checkNotNull(bo.conversations.getById(group.id()))
                    roundTrip(mine, peer, alix.inboxId(), bo.inboxId(), catalogue)

                    val denied = MetadataEditorController(peer, bo.inboxId(), { true }, catalogue)
                    denied.refresh()
                    denied.edit(MetadataEdit.Scalar(id(0xc008), "denied"))
                    assertNotNull(denied.failure)
                    assertNotNull(denied.state.value.error)
                    group.sync()
                    assertNull(group.metadataValue(ref(0xc008)))
                    println("METADATA_PROOF group denied=${denied.failure} committed=absent")

                    val dm = alix.conversations.createDm(bo.inboxId())
                    bo.conversations.sync()
                    ownRoundTrip(
                        Conversation.Dm(dm),
                        checkNotNull(bo.conversations.getById(dm.id())),
                        alix.inboxId(),
                        bo.inboxId(),
                        catalogue,
                    )
                    assertEquals(
                        FieldValue.String(""),
                        mine
                            .userData(listOf(ref(0xc006)), listOf(alix.inboxId()))
                            .getValue(alix.inboxId())
                            .single()
                            .value,
                    )
                } finally {
                    withContext(NonCancellable) {
                        clients.asReversed().forEach { it.end() }
                        paths.forEach { it.deleteRecursively() }
                        AndroidStreamLifecycle.enabled = lifecycle
                    }
                }
            }
        }

    private suspend fun roundTrip(
        source: Conversation,
        peer: Conversation,
        own: String,
        other: String,
        catalogue: List<ApplicationComponentDefinition>,
    ) {
        val editor = MetadataEditorController(source, own, { true }, catalogue)
        editor.refresh()

        suspend fun edit(change: MetadataEdit) {
            editor.edit(change)
            assertNull(editor.state.value.error)
            peer.sync()
        }
        edit(MetadataEdit.Scalar(id(0xc001), ""))
        assertEquals(MetadataValue.Scalar(FieldValue.String("")), peer.metadataValue(ref(0xc001)))
        edit(MetadataEdit.Scalar(id(0xc001), "Title"))
        assertEquals(MetadataValue.Scalar(FieldValue.String("Title")), peer.metadataValue(ref(0xc001)))
        edit(MetadataEdit.Scalar(id(0xc001), null))
        assertNull(peer.metadataValue(ref(0xc001)))
        edit(MetadataEdit.Scalar(id(0xc002), ""))
        assertArrayEquals(byteArrayOf(), bytes(peer, 0xc002))
        edit(MetadataEdit.Scalar(id(0xc002), "0080ff"))
        assertArrayEquals(byteArrayOf(0, -128, -1), bytes(peer, 0xc002))
        edit(MetadataEdit.Scalar(id(0xc002), null))
        assertNull(peer.metadataValue(ref(0xc002)))

        edit(MetadataEdit.Entry(id(0xc003), EntryAction.INSERT, "", ""))
        edit(MetadataEdit.Entry(id(0xc003), EntryAction.INSERT, "01", "00ff"))
        edit(MetadataEdit.Entry(id(0xc003), EntryAction.INSERT, "02", "7f"))
        edit(MetadataEdit.Entry(id(0xc003), EntryAction.UPDATE, "01", "80"))
        assertArrayEquals(
            byteArrayOf(127),
            (peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(2))) as FieldValue.Bytes).v1,
        )
        assertArrayEquals(
            byteArrayOf(-128),
            (peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(1))) as FieldValue.Bytes).v1,
        )
        assertArrayEquals(
            byteArrayOf(),
            (peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf())) as FieldValue.Bytes).v1,
        )
        edit(MetadataEdit.Entry(id(0xc003), EntryAction.DELETE, "01"))
        assertNull(peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(1))))
        assertArrayEquals(
            byteArrayOf(127),
            (peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(2))) as FieldValue.Bytes).v1,
        )

        edit(MetadataEdit.Entry(id(0xc004), EntryAction.INSERT, "00ff"))
        edit(MetadataEdit.Entry(id(0xc004), EntryAction.INSERT, "80"))
        val set = (peer.metadataValue(ref(0xc004)) as MetadataValue.Set).v1.map { MetadataMapper.key(it) }.sorted()
        assertEquals(listOf("00ff", "80"), set)
        edit(MetadataEdit.Entry(id(0xc004), EntryAction.DELETE, "00ff"))
        assertArrayEquals(
            byteArrayOf(-128),
            ((peer.metadataValue(ref(0xc004)) as MetadataValue.Set).v1.single() as FieldKey.Bytes).v1,
        )
        edit(MetadataEdit.Entry(id(0xc004), EntryAction.DELETE, "80"))
        assertTrue((peer.metadataValue(ref(0xc004)) as MetadataValue.Set).v1.isEmpty())
        edit(MetadataEdit.Entry(id(0xc005), EntryAction.INSERT, other))
        edit(MetadataEdit.Entry(id(0xc005), EntryAction.INSERT, own))
        edit(MetadataEdit.Entry(id(0xc005), EntryAction.DELETE, own))
        assertEquals(FieldKey.InboxId(other), (peer.metadataValue(ref(0xc005)) as MetadataValue.Set).v1.single())
        edit(MetadataEdit.Entry(id(0xc005), EntryAction.DELETE, other))
        assertTrue((peer.metadataValue(ref(0xc005)) as MetadataValue.Set).v1.isEmpty())

        ownRoundTrip(source, peer, own, other, catalogue)
    }

    private suspend fun ownRoundTrip(
        source: Conversation,
        peer: Conversation,
        own: String,
        other: String,
        catalogue: List<ApplicationComponentDefinition>,
    ) {
        val editor = MetadataEditorController(source, own, { true }, catalogue)

        suspend fun edit(change: MetadataEdit) {
            editor.edit(change)
            assertNull(editor.state.value.error)
            peer.sync()
        }
        peer.updateUserData(
            listOf(
                UserFieldUpdate(ref(0xc006), FieldValue.String("Bo")),
                UserFieldUpdate(ref(0xc007), FieldValue.Bytes(byteArrayOf(127))),
            ),
        )
        source.sync()
        editor.refresh()
        edit(MetadataEdit.Own(mapOf(id(0xc006) to "Alix", id(0xc007) to "00ff")))
        val readback = peer.userData(listOf(ref(0xc006), ref(0xc007)), listOf(own, other))
        val values = readback.getValue(own).associate { it.field.componentId to it.value }
        assertEquals(FieldValue.String("Alix"), values[0xc006.toUShort()])
        assertArrayEquals(byteArrayOf(0, -1), (values[0xc007.toUShort()] as FieldValue.Bytes).v1)
        val siblings = readback.getValue(other).associate { it.field.componentId to it.value }
        assertEquals(FieldValue.String("Bo"), siblings[0xc006.toUShort()])
        assertArrayEquals(byteArrayOf(127), (siblings[0xc007.toUShort()] as FieldValue.Bytes).v1)
        edit(MetadataEdit.Own(mapOf(id(0xc006) to "", id(0xc007) to "")))
        val empty = peer.userData(listOf(ref(0xc006), ref(0xc007)), listOf(own)).getValue(own)
        assertEquals(FieldValue.String(""), empty.single { it.field.componentId == 0xc006.toUShort() }.value)
        assertArrayEquals(
            byteArrayOf(),
            (
                empty
                    .single {
                        it.field.componentId == 0xc007.toUShort()
                    }.value as FieldValue.Bytes
            ).v1,
        )
        edit(MetadataEdit.Own(mapOf(id(0xc007) to null)))
        assertFalse(peer.userData(listOf(ref(0xc007)), listOf(own)).getValue(own).isNotEmpty())

        val fence = SessionFence()
        val key = checkNotNull(fence.replace("metadata-profile"))
        var screen = 1L
        val stale = MetadataEditorController(source, own, { fence.accepts(key) && screen == 1L }, catalogue)
        stale.refresh()
        screen++
        stale.edit(MetadataEdit.Own(mapOf(id(0xc006) to "old screen")))
        screen = 1L
        fence.replace("other-profile")
        stale.edit(MetadataEdit.Own(mapOf(id(0xc006) to "old session")))
        peer.sync()
        assertEquals(
            FieldValue.String(""),
            peer
                .userData(listOf(ref(0xc006)), listOf(own))
                .getValue(own)
                .single()
                .value,
        )
        val actualOwn = readback.mapValues { (_, rows) -> rows.map { MetadataMapper.scalar(it.value) } }
        println(
            "METADATA_PROOF ${source.id()} own=$actualOwn " +
                "empty=present clear=absent stale=blocked",
        )
    }
}

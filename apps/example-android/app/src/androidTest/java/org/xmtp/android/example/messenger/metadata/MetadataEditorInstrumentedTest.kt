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

    private suspend fun mapBytes(
        chat: Conversation,
        key: ByteArray,
    ): ByteArray {
        val value = chat.mapValue(ref(0xc003), FieldKey.Bytes(key))
        assertTrue("Map key ${MetadataMapper.hex(key)} must retain its Bytes value", value is FieldValue.Bytes)
        return (value as FieldValue.Bytes).v1
    }

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
                    assertEquals(
                        ((0xc001..0xc008) + (0xfd00..0xfd03)).map { it.toUShort() },
                        catalogue.map { it.componentId },
                    )
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
                    immutableFields(mine, peer, alix.inboxId(), bo.inboxId(), catalogue, group = true)

                    val denied = MetadataEditorController(peer, bo.inboxId(), { true }, catalogue)
                    denied.refresh()
                    denied.beforeWrite = {
                        group.updateMetadataField(
                            ref(0xc008),
                            ComponentMutation.Replace(FieldValue.String("Committed")),
                        )
                        peer.sync()
                    }
                    denied.edit(MetadataEdit.Scalar(id(0xc008), "denied"))
                    assertNotNull(denied.failure)
                    assertNotNull(denied.state.value.error)
                    group.sync()
                    assertEquals(MetadataValue.Scalar(FieldValue.String("Committed")), group.metadataValue(ref(0xc008)))
                    assertEquals(
                        "Committed",
                        denied.state.value.fields
                            .single { it.id == id(0xc008) }
                            .scalar,
                    )
                    println("METADATA_PROOF group denied=${denied.failure} refreshed=Committed")

                    val dm = alix.conversations.createDm(bo.inboxId())
                    bo.conversations.sync()
                    ownRoundTrip(
                        Conversation.Dm(dm),
                        checkNotNull(bo.conversations.getById(dm.id())),
                        alix.inboxId(),
                        bo.inboxId(),
                        catalogue,
                    )
                    immutableFields(
                        Conversation.Dm(dm),
                        checkNotNull(bo.conversations.getById(dm.id())),
                        alix.inboxId(),
                        bo.inboxId(),
                        catalogue,
                        group = false,
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

    private suspend fun immutableFields(
        source: Conversation,
        peer: Conversation,
        own: String,
        other: String,
        catalogue: List<ApplicationComponentDefinition>,
        group: Boolean,
    ) {
        val editor = MetadataEditorController(source, own, { true }, catalogue)
        editor.refresh()
        var writes = 0
        editor.beforeWrite = { writes++ }

        suspend fun save(edit: MetadataEdit) {
            editor.edit(edit)
            assertNull(editor.failure)
            peer.sync()
        }

        suspend fun reject(edit: MetadataEdit) {
            editor.edit(edit)
            assertNotNull(editor.failure)
        }
        if (group) {
            save(MetadataEdit.Scalar(id(0xfd00), "First"))
            assertFalse(
                editor.state.value.fields
                    .single { it.id == id(0xfd00) }
                    .canWrite,
            )
            reject(MetadataEdit.Scalar(id(0xfd00), "Later"))
            reject(MetadataEdit.Scalar(id(0xfd00), null))
            assertEquals(1, writes)
            assertNotNull(
                runCatching {
                    source.updateMetadataField(ref(0xfd00), ComponentMutation.Replace(FieldValue.String("SDK rejects")))
                }.exceptionOrNull(),
            )
            peer.sync()
            assertEquals(MetadataValue.Scalar(FieldValue.String("First")), peer.metadataValue(ref(0xfd00)))
            save(MetadataEdit.Entry(id(0xfd01), EntryAction.INSERT, "01", "02"))
            reject(MetadataEdit.Entry(id(0xfd01), EntryAction.INSERT, "03", "04"))
            reject(MetadataEdit.Entry(id(0xfd01), EntryAction.DELETE, "01"))
            assertArrayEquals(
                byteArrayOf(2),
                (peer.mapValue(ref(0xfd01), FieldKey.Bytes(byteArrayOf(1))) as FieldValue.Bytes).v1,
            )
            assertNull(peer.mapValue(ref(0xfd01), FieldKey.Bytes(byteArrayOf(3))))
            save(MetadataEdit.Entry(id(0xfd02), EntryAction.INSERT, "00ff"))
            reject(MetadataEdit.Entry(id(0xfd02), EntryAction.INSERT, "80"))
            assertArrayEquals(
                byteArrayOf(0, -1),
                ((peer.metadataValue(ref(0xfd02)) as MetadataValue.Set).v1.single() as FieldKey.Bytes).v1,
            )
        }
        save(MetadataEdit.Own(mapOf(id(0xfd03) to "First own")))
        val otherEditor = MetadataEditorController(peer, other, { true }, catalogue)
        otherEditor.refresh()
        var peerWrites = 0
        otherEditor.beforeWrite = { peerWrites++ }
        val readonly =
            otherEditor.state.value.fields
                .single { it.id == id(0xfd03) }
        assertFalse(readonly.present)
        assertTrue(readonly.componentPresent)
        assertFalse(readonly.canWrite)
        otherEditor.edit(MetadataEdit.Own(mapOf(id(0xfd03) to "Later peer")))
        assertNotNull(otherEditor.failure)
        assertEquals(0, peerWrites)
        source.sync()
        assertEquals(
            FieldValue.String("First own"),
            source
                .userData(listOf(ref(0xfd03)), listOf(own))
                .getValue(own)
                .single()
                .value,
        )
        assertTrue(source.userData(listOf(ref(0xfd03)), listOf(other)).getValue(other).isEmpty())
        println("IMMUTABLE_PROOF ${source.id()} first=retained later=blocked peer-own=absent")
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
            mapBytes(peer, byteArrayOf(2)),
        )
        assertArrayEquals(
            byteArrayOf(-128),
            mapBytes(peer, byteArrayOf(1)),
        )
        assertArrayEquals(
            byteArrayOf(),
            mapBytes(peer, byteArrayOf()),
        )
        edit(MetadataEdit.Entry(id(0xc003), EntryAction.DELETE, "01"))
        assertNull(peer.mapValue(ref(0xc003), FieldKey.Bytes(byteArrayOf(1))))
        assertArrayEquals(
            byteArrayOf(127),
            mapBytes(peer, byteArrayOf(2)),
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
        val initial =
            peer
                .userData(listOf(ref(0xc006), ref(0xc007)), listOf(own))
                .getValue(own)
                .associate { it.field.componentId to it.value }
        assertEquals(FieldValue.String("Alix"), initial[0xc006.toUShort()])
        assertTrue(initial[0xc007.toUShort()] is FieldValue.Bytes)
        assertArrayEquals(byteArrayOf(0, -1), (initial[0xc007.toUShort()] as FieldValue.Bytes).v1)
        val displayed = OwnFieldDraft().merge(editor.state.value.fields)
        val dirty = displayed.change(id(0xc007), "80").edit()
        source.updateUserData(listOf(UserFieldUpdate(ref(0xc006), FieldValue.String("New Alix"))))
        edit(dirty)
        val concurrent = peer.userData(listOf(ref(0xc006), ref(0xc007)), listOf(own)).getValue(own)
        assertEquals(
            FieldValue.String("New Alix"),
            concurrent.single { it.field.componentId == 0xc006.toUShort() }.value,
        )
        assertArrayEquals(
            byteArrayOf(-128),
            (concurrent.single { it.field.componentId == 0xc007.toUShort() }.value as FieldValue.Bytes).v1,
        )
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
        stale.beforeWrite = { screen++ }
        stale.edit(MetadataEdit.Own(mapOf(id(0xc006) to "old screen")))
        peer.sync()
        assertEquals(
            FieldValue.String(""),
            peer
                .userData(listOf(ref(0xc006)), listOf(own))
                .getValue(own)
                .single()
                .value,
        )
        screen = 1L
        stale.beforeWrite = { fence.replace("other-profile") }
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

package org.xmtp.android.example.messenger.metadata

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*

class MetadataEditorTest {
    private val own = "a".repeat(64)
    private val peer = "b".repeat(64)
    private val allow = MetadataPolicy.Base(MetadataBasePolicy.Allow)

    private fun descriptor(
        id: Int,
        type: MetadataComponentType,
        user: Boolean = false,
    ) = MetadataFieldDescriptor(
        MetadataFieldRef(id.toUShort(), "Field $id"),
        type,
        ComponentPermissions(allow, allow, allow),
        user,
    )

    private class RecordingGroup : Group(NoHandle) {
        var fields = emptyList<MetadataFieldDescriptor>()
        var values = emptyList<MetadataFieldValue>()
        var users = emptyMap<String, List<UserFieldValue>>()
        val mutations = mutableListOf<Pair<MetadataFieldRef, ComponentMutation>>()
        val userWrites = mutableListOf<List<UserFieldUpdate>>()
        val reads = mutableListOf<List<MetadataFieldRef>>()
        var descriptorRead: (() -> Unit)? = null
        var denied: Exception? = null
        var afterWrite: (() -> Unit)? = null
        var afterUserWrite: ((List<UserFieldUpdate>) -> Unit)? = null

        override suspend fun metadataFields(): List<MetadataFieldDescriptor> {
            descriptorRead?.invoke()
            return fields
        }

        override suspend fun metadataValues(fields: List<MetadataFieldRef>): List<MetadataFieldValue> {
            reads += fields
            return values.filter { row ->
                fields.any {
                    it.componentId ==
                        row.field.componentId
                }
            }
        }

        override suspend fun metadataValue(field: MetadataFieldRef): MetadataValue? =
            values.singleOrNull { it.field.componentId == field.componentId }?.value
                ?: users
                    .flatMap { (inbox, fields) ->
                        fields
                            .filter { it.field.componentId == field.componentId }
                            .map { MapEntry(FieldKey.InboxId(inbox), it.value) }
                    }.takeIf { it.isNotEmpty() }
                    ?.let { MetadataValue.Map(it) }

        override suspend fun userData(
            fields: List<MetadataFieldRef>?,
            inboxIds: List<String>?,
        ) = users

        override suspend fun updateMetadataField(
            field: MetadataFieldRef,
            operation: ComponentMutation,
        ) {
            mutations +=
                field to operation
            afterWrite?.invoke()
            denied?.let { throw it }
        }

        override suspend fun updateUserData(values: List<UserFieldUpdate>) {
            userWrites += values
            afterUserWrite?.invoke(values)
        }
    }

    @Test fun unknownTypesAreNotReadAndLabelsDoNotChangeIdentity() =
        runBlocking {
            val group = RecordingGroup()
            val field = descriptor(0xc001, MetadataComponentType.Bytes)
            group.fields = listOf(field, descriptor(0xc002, MetadataComponentType.Unknown(99)))
            group.values =
                listOf(
                    MetadataFieldValue(
                        MetadataFieldRef(field.field.componentId, "Different label"),
                        MetadataValue.Scalar(FieldValue.Bytes(byteArrayOf(0, -1))),
                    ),
                )
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            assertEquals(listOf(0xc001.toUShort()), group.reads.single().map { it.componentId })
            assertEquals(
                "00ff",
                controller.state.value.fields
                    .first()
                    .scalar,
            )
            assertFalse(
                controller.state.value.fields
                    .last()
                    .editable,
            )
            group.fields = listOf(field.copy(field = field.field.copy(name = "Renamed")))
            controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "ff00"))
            assertArrayEquals(
                byteArrayOf(-1, 0),
                ((group.mutations.single().second as ComponentMutation.Replace).v1 as FieldValue.Bytes).v1,
            )
        }

    @Test fun mapEditUsesOnlyOneKeyDelta() =
        runBlocking {
            val group = RecordingGroup()
            val field = descriptor(0xc003, MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.BYTES))
            group.fields = listOf(field)
            group.values =
                listOf(
                    MetadataFieldValue(
                        field.field,
                        MetadataValue.Map(
                            listOf(
                                MapEntry(FieldKey.Bytes(byteArrayOf(1)), FieldValue.Bytes(byteArrayOf(2))),
                                MapEntry(FieldKey.Bytes(byteArrayOf(3)), FieldValue.Bytes(byteArrayOf(4))),
                            ),
                        ),
                    ),
                )
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            controller.edit(MetadataEdit.Entry(FieldUiId(field.field.componentId), EntryAction.UPDATE, "01", "00ff"))
            val delta = group.mutations.single().second as ComponentMutation.MapDelta
            assertEquals(1, delta.v1.size)
            val update = delta.v1.single() as MapMutation.Update
            assertArrayEquals(byteArrayOf(1), (update.v1 as FieldKey.Bytes).v1)
            assertArrayEquals(byteArrayOf(0, -1), (update.v2 as FieldValue.Bytes).v1)
        }

    @Test fun ownSaveBatchesOnlyChangedRefsAndSeparatesEmptyFromClear() =
        runBlocking {
            val group = RecordingGroup()
            val first =
                descriptor(0xc006, MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING), true)
            val second =
                descriptor(0xc007, MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.BYTES), true)
            group.fields = listOf(first, second)
            group.users =
                mapOf(
                    own to
                        listOf(
                            UserFieldValue(first.field, FieldValue.String("")),
                            UserFieldValue(second.field, FieldValue.Bytes(byteArrayOf(1))),
                        ),
                    peer to listOf(UserFieldValue(first.field, FieldValue.String("peer"))),
                )
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            controller.edit(
                MetadataEdit.Own(
                    mapOf(
                        FieldUiId(first.field.componentId) to "",
                        FieldUiId(second.field.componentId) to "",
                    ),
                ),
            )
            assertEquals(1, group.userWrites.size)
            assertEquals(
                second.field.componentId,
                group.userWrites
                    .single()
                    .single()
                    .field.componentId,
            )
            assertArrayEquals(
                byteArrayOf(),
                (
                    group.userWrites
                        .single()
                        .single()
                        .value as FieldValue.Bytes
                ).v1,
            )
            controller.edit(MetadataEdit.Own(mapOf(FieldUiId(first.field.componentId) to null)))
            assertNull(
                group.userWrites
                    .last()
                    .single()
                    .value,
            )
            assertEquals(
                "peer",
                controller.state.value.members
                    .single()
                    .values
                    .first()
                    .scalar,
            )
        }

    @Test fun staleUntouchedOwnValueCannotOverwriteConcurrentUpdate() =
        runBlocking {
            val group = RecordingGroup()
            val stringShape = MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING)
            val byteShape = MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.BYTES)
            val first = descriptor(0xc006, stringShape, true)
            val second = descriptor(0xc007, byteShape, true)
            val firstId = FieldUiId(first.field.componentId)
            val secondId = FieldUiId(second.field.componentId)
            group.fields = listOf(first, second)
            group.users =
                mapOf(
                    own to
                        listOf(
                            UserFieldValue(first.field, FieldValue.String("old")),
                            UserFieldValue(second.field, FieldValue.Bytes(byteArrayOf(1))),
                        ),
                )
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            val displayed = OwnFieldDraft().merge(controller.state.value.fields)
            val submitted = displayed.change(secondId, "02").edit()
            group.descriptorRead = {
                group.descriptorRead = null
                group.users =
                    mapOf(
                        own to
                            listOf(
                                UserFieldValue(first.field, FieldValue.String("new")),
                                UserFieldValue(second.field, FieldValue.Bytes(byteArrayOf(1))),
                            ),
                    )
            }
            group.afterUserWrite = { changes ->
                val committed =
                    group.users
                        .getValue(own)
                        .associateBy { it.field.componentId }
                        .toMutableMap()
                for (change in changes) {
                    if (change.value == null) {
                        committed.remove(change.field.componentId)
                    } else {
                        committed[change.field.componentId] = UserFieldValue(change.field, checkNotNull(change.value))
                    }
                }
                group.users = group.users + (own to committed.values.toList())
            }
            controller.edit(submitted)
            assertNull(controller.failure)
            val saved = group.users.getValue(own).associate { it.field.componentId to it.value }
            assertEquals(FieldValue.String("new"), saved[firstId.componentId])
            assertEquals(listOf(second.field.componentId), group.userWrites.single().map { it.field.componentId })
            assertArrayEquals(byteArrayOf(2), (saved[secondId.componentId] as FieldValue.Bytes).v1)

            group.descriptorRead = null
            val refreshed = OwnFieldDraft().merge(controller.state.value.fields)
            val next = refreshed.change(secondId, "03").edit()
            group.users =
                mapOf(
                    own to
                        listOf(
                            UserFieldValue(first.field, FieldValue.String("new")),
                            UserFieldValue(second.field, FieldValue.Bytes(byteArrayOf(3))),
                        ),
                )
            controller.edit(next)
            assertNull(controller.failure)
            assertEquals(1, group.userWrites.size)
        }

    @Test fun ownDraftRefreshKeepsDirtyInputsAndMergesUntouchedValues() {
        val first =
            FieldUi(
                FieldUiId(0xc006.toUShort()),
                "A",
                FieldShape.USER_STRING,
                "Own",
                true,
                scalar = "old",
                userField = true,
            )
        val second =
            FieldUi(
                FieldUiId(0xc007.toUShort()),
                "B",
                FieldShape.USER_BYTES,
                "Own",
                true,
                scalar = "01",
                userField = true,
            )
        val displayed = OwnFieldDraft().merge(listOf(first, second))
        val dirty = displayed.change(second.id, "02")
        val fresh = dirty.merge(listOf(first.copy(scalar = "new"), second))
        assertEquals("new", fresh.values[first.id])
        assertEquals("02", fresh.values[second.id])
        assertEquals(mapOf(second.id to "02"), fresh.edit().values)
        val committed = fresh.merge(listOf(first.copy(scalar = "new"), second.copy(scalar = "02")))
        assertTrue(committed.edit().values.isEmpty())

        val clear = displayed.change(first.id, null)
        assertEquals(mapOf(first.id to null), clear.merge(listOf(first, second)).edit().values)
        val empty = displayed.change(first.id, "")
        assertEquals(mapOf(first.id to ""), empty.merge(listOf(first, second)).edit().values)
    }

    @Test fun immutableGroupFieldsPermitOnlyTheirInitialValue() =
        runBlocking {
            val shapes =
                listOf(
                    MetadataComponentType.String,
                    MetadataComponentType.Bytes,
                    MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.BYTES),
                    MetadataComponentType.Set(MetadataKeyType.BYTES),
                    MetadataComponentType.Set(MetadataKeyType.INBOX_ID),
                )
            for ((index, shape) in shapes.withIndex()) {
                val group = RecordingGroup()
                val field = descriptor(0xfd00 + index, shape)
                val id = FieldUiId(field.field.componentId)
                group.fields = listOf(field)
                val controller = MetadataEditorController(Conversation.Group(group), own, { true })
                controller.refresh()
                assertTrue(
                    controller.state.value.fields
                        .single()
                        .canWrite,
                )
                val insert =
                    when (shape) {
                        MetadataComponentType.String -> {
                            MetadataEdit.Scalar(id, "initial")
                        }

                        MetadataComponentType.Bytes -> {
                            MetadataEdit.Scalar(id, "00ff")
                        }

                        is MetadataComponentType.Map -> {
                            MetadataEdit.Entry(id, EntryAction.INSERT, "01", "02")
                        }

                        is MetadataComponentType.Set -> {
                            MetadataEdit.Entry(
                                id,
                                EntryAction.INSERT,
                                if (shape.keyType == MetadataKeyType.INBOX_ID) peer else "01",
                            )
                        }

                        else -> {
                            error("Unexpected test shape")
                        }
                    }
                val invalidAbsent =
                    if (insert is MetadataEdit.Scalar) {
                        MetadataEdit.Scalar(id, null)
                    } else {
                        MetadataEdit.Entry(id, EntryAction.DELETE, "01")
                    }
                controller.edit(invalidAbsent)
                assertTrue(group.mutations.isEmpty())
                controller.edit(insert)
                assertNull(controller.failure)
                assertEquals(1, group.mutations.size)
                val value =
                    when (shape) {
                        MetadataComponentType.String -> {
                            MetadataValue.Scalar(FieldValue.String("initial"))
                        }

                        MetadataComponentType.Bytes -> {
                            MetadataValue.Scalar(FieldValue.Bytes(byteArrayOf(0, -1)))
                        }

                        is MetadataComponentType.Map -> {
                            MetadataValue.Map(
                                listOf(
                                    MapEntry(FieldKey.Bytes(byteArrayOf(1)), FieldValue.Bytes(byteArrayOf(2))),
                                ),
                            )
                        }

                        is MetadataComponentType.Set -> {
                            MetadataValue.Set(
                                listOf(
                                    if (shape.keyType == MetadataKeyType.INBOX_ID) {
                                        FieldKey.InboxId(peer)
                                    } else {
                                        FieldKey.Bytes(byteArrayOf(1))
                                    },
                                ),
                            )
                        }

                        else -> {
                            error("Unexpected test shape")
                        }
                    }
                group.values = listOf(MetadataFieldValue(field.field, value))
                controller.refresh()
                val rendered =
                    controller.state.value.fields
                        .single()
                assertTrue(rendered.editable)
                assertTrue(rendered.immutable)
                assertFalse(rendered.canWrite)
                controller.edit(insert)
                assertNotNull(controller.failure)
                assertEquals(1, group.mutations.size)
                controller.edit(invalidAbsent)
                assertEquals(1, group.mutations.size)
            }
            assertFalse(MetadataMapper.field(descriptor(0xfcff, MetadataComponentType.String), null).immutable)
            assertTrue(MetadataMapper.field(descriptor(0xfeff, MetadataComponentType.String), null).immutable)
        }

    @Test fun immutableOwnMapsLockForAllMembersAfterTheFirstEntry() =
        runBlocking {
            for (type in listOf(MetadataScalarType.STRING, MetadataScalarType.BYTES)) {
                val group = RecordingGroup()
                val field = descriptor(0xfd03, MetadataComponentType.Map(MetadataKeyType.INBOX_ID, type), true)
                val id = FieldUiId(field.field.componentId)
                group.fields = listOf(field)
                val controller = MetadataEditorController(Conversation.Group(group), own, { true })
                controller.refresh()
                controller.edit(
                    MetadataEdit.Own(
                        mapOf(id to if (type == MetadataScalarType.STRING) "initial" else "00ff"),
                    ),
                )
                assertNull(controller.failure)
                assertEquals(1, group.userWrites.size)
                val peerValue =
                    if (type == MetadataScalarType.STRING) {
                        FieldValue.String("peer")
                    } else {
                        FieldValue.Bytes(byteArrayOf(1))
                    }
                group.users = mapOf(peer to listOf(UserFieldValue(field.field, peerValue)))
                group.values =
                    listOf(
                        MetadataFieldValue(
                            field.field,
                            MetadataValue.Map(listOf(MapEntry(FieldKey.InboxId(peer), peerValue))),
                        ),
                    )
                controller.refresh()
                val rendered =
                    controller.state.value.fields
                        .single()
                assertFalse(rendered.present)
                assertTrue(rendered.componentPresent)
                assertFalse(rendered.canWrite)
                assertTrue(OwnFieldDraft().merge(controller.state.value.fields).values.isEmpty())
                controller.edit(MetadataEdit.Own(mapOf(id to "02")))
                assertNotNull(controller.failure)
                assertEquals(1, group.userWrites.size)
                assertEquals(
                    peerValue,
                    group.users
                        .getValue(peer)
                        .single()
                        .value,
                )
            }
        }

    @Test fun changedTypeOrPolicyRejectsStaleFormAndRefreshes() =
        runBlocking {
            for (policyChange in listOf(false, true)) {
                val group = RecordingGroup()
                val field = descriptor(0xc001, MetadataComponentType.String)
                group.fields = listOf(field)
                val controller = MetadataEditorController(Conversation.Group(group), own, { true })
                controller.refresh()
                group.fields =
                    listOf(
                        if (policyChange) {
                            field.copy(
                                permissions =
                                    field.permissions.copy(
                                        update = MetadataPolicy.Base(MetadataBasePolicy.AllowIfAdmin),
                                    ),
                            )
                        } else {
                            field.copy(componentType = MetadataComponentType.Bytes)
                        },
                    )
                controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "old form"))
                assertTrue(group.mutations.isEmpty())
                assertTrue(
                    controller.state.value.error
                        .orEmpty()
                        .contains("type or policy changed"),
                )
                assertEquals(
                    if (policyChange) FieldShape.STRING else FieldShape.BYTES,
                    controller.state.value.fields
                        .single()
                        .shape,
                )
            }
        }

    @Test fun oldSessionOrConversationCannotDisplayOrCommit() =
        runBlocking {
            val group = RecordingGroup()
            val field = descriptor(0xc001, MetadataComponentType.String)
            group.fields = listOf(field)
            var current = true
            val controller = MetadataEditorController(Conversation.Group(group), own, { current })
            controller.refresh()
            val before = controller.state.value.fields
            group.descriptorRead = { current = false }
            group.values = listOf(MetadataFieldValue(field.field, MetadataValue.Scalar(FieldValue.String("stale"))))
            controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "do not commit"))
            assertTrue(group.mutations.isEmpty())
            assertEquals(before, controller.state.value.fields)
        }

    @Test fun deniedWriteRetainsTypedErrorAndRereadsValue() =
        runBlocking {
            val group = RecordingGroup()
            val field = descriptor(0xc001, MetadataComponentType.String)
            group.fields = listOf(field)
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            val denial =
                XmtpException.PermissionDenied(
                    ErrorDetails("PermissionDenied", ErrorCategory.INPUT, false, "Denied"),
                )
            group.denied = denial
            group.afterWrite = {
                group.values =
                    listOf(MetadataFieldValue(field.field, MetadataValue.Scalar(FieldValue.String("committed"))))
            }
            controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "uncommitted"))
            assertSame(denial, controller.failure)
            assertEquals(
                "committed",
                controller.state.value.fields
                    .single()
                    .scalar,
            )
            assertTrue(
                controller.state.value.error
                    .orEmpty()
                    .contains("PermissionDenied"),
            )
        }

    @Test fun validationUsesBytesAndSerializedCollectionBounds() {
        assertArrayEquals(byteArrayOf(0, -128, -1), MetadataMapper.bytes("0080ff"))
        for (text in listOf(
            "f",
            "xx",
            "00".repeat(8193),
        )) {
            assertTrue(runCatching { MetadataMapper.bytes(text) }.isFailure)
        }
        assertTrue(runCatching { MetadataMapper.inbox("A".repeat(64)) }.isFailure)
        assertTrue(runCatching { MetadataMapper.value(FieldShape.STRING, "é".repeat(4097)) }.isFailure)
        val oversized = (0..7).map { FieldEntry("%02x".format(it), "00".repeat(8192)) }
        assertTrue(runCatching { MetadataMapper.collectionSize(FieldShape.BYTE_MAP, oversized) }.isFailure)
        MetadataMapper.collectionSize(FieldShape.BYTE_MAP, listOf(FieldEntry("", "")))
    }

    @Test fun unsupportedMapAndPolicyAreNotReadOrWritten() =
        runBlocking {
            val group = RecordingGroup()
            val map = descriptor(0xc003, MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.STRING))
            val unknown = MetadataPolicy.Base(MetadataBasePolicy.Unknown(99))
            val policy =
                descriptor(0xc001, MetadataComponentType.String).let {
                    it.copy(permissions = it.permissions.copy(update = unknown))
                }
            group.fields = listOf(map, policy)
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            assertTrue(group.reads.isEmpty())
            assertTrue(
                controller.state.value.fields
                    .all { !it.editable },
            )
            controller.edit(MetadataEdit.Scalar(FieldUiId(policy.field.componentId), "blocked"))
            assertTrue(group.mutations.isEmpty())
            assertTrue(
                controller.state.value.error
                    .orEmpty()
                    .contains("Unsupported"),
            )
        }

    @Test fun offeredMissingFieldDoesNotCreateARegistryEntry() =
        runBlocking {
            val group = RecordingGroup()
            val field = descriptor(0xc001, MetadataComponentType.String)
            val offered =
                ApplicationComponentDefinition(
                    field.field.componentId,
                    "Offered",
                    field.componentType,
                    field.permissions,
                    true,
                    true,
                )
            val controller = MetadataEditorController(Conversation.Group(group), own, { true }, listOf(offered))
            controller.refresh()
            assertTrue(
                controller.state.value.fields
                    .isEmpty(),
            )
            assertEquals(listOf("Offered"), controller.state.value.missing)
            controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "unregistered"))
            assertTrue(group.mutations.isEmpty())
            assertNotNull(controller.failure)
        }

    @Test fun ownBoundsIncludeEntriesOutsideCurrentMembership() =
        runBlocking {
            val group = RecordingGroup()
            val shape = MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.BYTES)
            val field = descriptor(0xc006, shape, true)
            group.fields = listOf(field)
            val entries =
                (0..6).map {
                    MapEntry(FieldKey.InboxId(it.toString(16).padStart(64, '0')), FieldValue.Bytes(ByteArray(8192)))
                }
            group.values = listOf(MetadataFieldValue(field.field, MetadataValue.Map(entries)))
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            controller.edit(MetadataEdit.Own(mapOf(FieldUiId(field.field.componentId) to "00".repeat(8192))))
            assertTrue(group.userWrites.isEmpty())
            assertTrue(
                controller.state.value.error
                    .orEmpty()
                    .contains("65536"),
            )
        }
}

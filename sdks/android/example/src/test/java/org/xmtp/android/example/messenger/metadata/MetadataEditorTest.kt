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
    private fun descriptor(id: Int, type: MetadataComponentType, user: Boolean = false) = MetadataFieldDescriptor(MetadataFieldRef(id.toUShort(), "Field $id"), type, ComponentPermissions(allow, allow, allow), user)

    private class RecordingGroup : Group(NoHandle) {
        var fields = emptyList<MetadataFieldDescriptor>()
        var values = emptyList<MetadataFieldValue>()
        var users = emptyMap<String, List<UserFieldValue>>()
        val mutations = mutableListOf<Pair<MetadataFieldRef, ComponentMutation>>()
        val userWrites = mutableListOf<List<UserFieldUpdate>>()
        val reads = mutableListOf<List<MetadataFieldRef>>()
        var descriptorRead: (() -> Unit)? = null
        var denied: Exception? = null
        override suspend fun metadataFields(): List<MetadataFieldDescriptor> { descriptorRead?.invoke(); return fields }
        override suspend fun metadataValues(fields: List<MetadataFieldRef>): List<MetadataFieldValue> { reads += fields; return values.filter { row -> fields.any { it.componentId == row.field.componentId } } }
        override suspend fun metadataValue(field: MetadataFieldRef): MetadataValue? = values.singleOrNull { it.field.componentId == field.componentId }?.value ?: MetadataValue.Map(users.flatMap { (inbox, fields) -> fields.filter { it.field.componentId == field.componentId }.map { MapEntry(FieldKey.InboxId(inbox), it.value) } })
        override suspend fun userData(fields: List<MetadataFieldRef>?, inboxIds: List<String>?) = users
        override suspend fun updateMetadataField(field: MetadataFieldRef, operation: ComponentMutation) { mutations += field to operation; denied?.let { throw it } }
        override suspend fun updateUserData(values: List<UserFieldUpdate>) { userWrites += values }
    }

    @Test fun unknownTypesAreNotReadAndLabelsDoNotChangeIdentity() = runBlocking {
        val group = RecordingGroup()
        val field = descriptor(0xc001, MetadataComponentType.Bytes)
        group.fields = listOf(field, descriptor(0xc002, MetadataComponentType.Unknown(99)))
        group.values = listOf(MetadataFieldValue(MetadataFieldRef(field.field.componentId, "Different label"), MetadataValue.Scalar(FieldValue.Bytes(byteArrayOf(0, -1)))))
        val controller = MetadataEditorController(Conversation.Group(group), own, { true })
        controller.refresh()
        assertEquals(listOf(0xc001.toUShort()), group.reads.single().map { it.componentId })
        assertEquals("00ff", controller.state.value.fields.first().scalar)
        assertFalse(controller.state.value.fields.last().editable)
        group.fields = listOf(field.copy(field = field.field.copy(name = "Renamed")))
        controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "ff00"))
        assertArrayEquals(byteArrayOf(-1, 0), ((group.mutations.single().second as ComponentMutation.Replace).v1 as FieldValue.Bytes).v1)
    }

    @Test fun mapEditUsesOnlyOneKeyDelta() = runBlocking {
        val group = RecordingGroup()
        val field = descriptor(0xc003, MetadataComponentType.Map(MetadataKeyType.BYTES, MetadataScalarType.BYTES))
        group.fields = listOf(field)
        group.values = listOf(MetadataFieldValue(field.field, MetadataValue.Map(listOf(MapEntry(FieldKey.Bytes(byteArrayOf(1)), FieldValue.Bytes(byteArrayOf(2))), MapEntry(FieldKey.Bytes(byteArrayOf(3)), FieldValue.Bytes(byteArrayOf(4)))))))
        val controller = MetadataEditorController(Conversation.Group(group), own, { true })
        controller.refresh()
        controller.edit(MetadataEdit.Entry(FieldUiId(field.field.componentId), EntryAction.UPDATE, "01", "00ff"))
        val delta = group.mutations.single().second as ComponentMutation.MapDelta
        assertEquals(1, delta.v1.size)
        val update = delta.v1.single() as MapMutation.Update
        assertArrayEquals(byteArrayOf(1), (update.v1 as FieldKey.Bytes).v1)
        assertArrayEquals(byteArrayOf(0, -1), (update.v2 as FieldValue.Bytes).v1)
    }

    @Test fun ownSaveBatchesOnlyChangedRefsAndSeparatesEmptyFromClear() = runBlocking {
        val group = RecordingGroup()
        val first = descriptor(0xc006, MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.STRING), true)
        val second = descriptor(0xc007, MetadataComponentType.Map(MetadataKeyType.INBOX_ID, MetadataScalarType.BYTES), true)
        group.fields = listOf(first, second)
        group.users = mapOf(own to listOf(UserFieldValue(first.field, FieldValue.String("")), UserFieldValue(second.field, FieldValue.Bytes(byteArrayOf(1)))), peer to listOf(UserFieldValue(first.field, FieldValue.String("peer"))))
        val controller = MetadataEditorController(Conversation.Group(group), own, { true })
        controller.refresh()
        controller.edit(MetadataEdit.Own(mapOf(FieldUiId(first.field.componentId) to "", FieldUiId(second.field.componentId) to "")))
        assertEquals(1, group.userWrites.size)
        assertEquals(second.field.componentId, group.userWrites.single().single().field.componentId)
        assertArrayEquals(byteArrayOf(), (group.userWrites.single().single().value as FieldValue.Bytes).v1)
        controller.edit(MetadataEdit.Own(mapOf(FieldUiId(first.field.componentId) to null)))
        assertNull(group.userWrites.last().single().value)
        assertEquals("peer", controller.state.value.members.single().values.first().scalar)
    }

    @Test fun changedTypeOrPolicyRejectsStaleFormAndRefreshes() = runBlocking {
        for (policyChange in listOf(false, true)) {
            val group = RecordingGroup()
            val field = descriptor(0xc001, MetadataComponentType.String)
            group.fields = listOf(field)
            val controller = MetadataEditorController(Conversation.Group(group), own, { true })
            controller.refresh()
            group.fields = listOf(if (policyChange) field.copy(permissions = field.permissions.copy(update = MetadataPolicy.Base(MetadataBasePolicy.AllowIfAdmin))) else field.copy(componentType = MetadataComponentType.Bytes))
            controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "old form"))
            assertTrue(group.mutations.isEmpty())
            assertTrue(controller.state.value.error.orEmpty().contains("type or policy changed"))
            assertEquals(if (policyChange) FieldShape.STRING else FieldShape.BYTES, controller.state.value.fields.single().shape)
        }
    }

    @Test fun oldSessionOrConversationCannotDisplayOrCommit() = runBlocking {
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

    @Test fun deniedWriteRetainsTypedErrorAndRereadsValue() = runBlocking {
        val group = RecordingGroup()
        val field = descriptor(0xc001, MetadataComponentType.String)
        group.fields = listOf(field)
        val controller = MetadataEditorController(Conversation.Group(group), own, { true })
        controller.refresh()
        val denial = XmtpException.PermissionDenied(ErrorDetails("PermissionDenied", ErrorCategory.INPUT, false, "Denied"))
        group.denied = denial
        group.values = listOf(MetadataFieldValue(field.field, MetadataValue.Scalar(FieldValue.String("committed"))))
        controller.edit(MetadataEdit.Scalar(FieldUiId(field.field.componentId), "uncommitted"))
        assertSame(denial, controller.failure)
        assertEquals("committed", controller.state.value.fields.single().scalar)
        assertTrue(controller.state.value.error.orEmpty().contains("PermissionDenied"))
    }

    @Test fun validationUsesBytesAndSerializedCollectionBounds() {
        assertArrayEquals(byteArrayOf(0, -128, -1), MetadataMapper.bytes("0080ff"))
        for (text in listOf("f", "xx", "00".repeat(8193))) assertTrue(runCatching { MetadataMapper.bytes(text) }.isFailure)
        assertTrue(runCatching { MetadataMapper.inbox("A".repeat(64)) }.isFailure)
        assertTrue(runCatching { MetadataMapper.value(FieldShape.STRING, "é".repeat(4097)) }.isFailure)
        val oversized = (0..7).map { FieldEntry("%02x".format(it), "00".repeat(8192)) }
        assertTrue(runCatching { MetadataMapper.collectionSize(FieldShape.BYTE_MAP, oversized) }.isFailure)
        MetadataMapper.collectionSize(FieldShape.BYTE_MAP, listOf(FieldEntry("", "")))
    }
}

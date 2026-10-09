package org.xmtp.android.example.messenger.metadata

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*

/** One open editor. The owner checks both session and conversation generation. */
class MetadataEditorController(
    private val conversation: Conversation,
    private val ownInboxId: String,
    private val isCurrent: () -> Boolean,
    private val offered: List<ApplicationComponentDefinition> = emptyList(),
    private val reads: Semaphore = Semaphore(4),
) {
    private val mutableState = MutableStateFlow(MetadataEditorState())
    val state: StateFlow<MetadataEditorState> = mutableState
    var failure: Throwable? = null
        private set
    internal var beforeWrite: suspend () -> Unit = {}

    private suspend fun write(block: suspend () -> Unit) {
        if (!isCurrent()) return
        beforeWrite()
        if (isCurrent()) block()
    }

    private val mutex = Mutex()
    private var descriptors: Map<UShort, MetadataFieldDescriptor> = emptyMap()

    private suspend fun <T> read(block: suspend () -> T): T = reads.withPermit { block() }

    suspend fun refresh() = mutex.withLock { perform { reload() } }

    suspend fun edit(request: MetadataEdit) {
        val edit = if (request is MetadataEdit.Own) request.copy(values = request.values.toMap()) else request
        mutex.withLock {
            perform {
                if (edit == MetadataEdit.Refresh) {
                    reload()
                    return@perform
                }
                val original = descriptors
                reload()
                if (!isCurrent()) return@perform
                val ids =
                    when (edit) {
                        is MetadataEdit.Scalar -> listOf(edit.id)
                        is MetadataEdit.Entry -> listOf(edit.id)
                        is MetadataEdit.Own -> edit.values.keys.toList()
                        MetadataEdit.Refresh -> emptyList()
                    }
                for (id in ids) {
                    val before = original[id.componentId] ?: error("The field was not loaded.")
                    val now =
                        descriptors[id.componentId] ?: error("The field is no longer registered. Reload the editor.")
                    val sameType = before.componentType == now.componentType
                    val samePolicy = before.permissions == now.permissions
                    val sameUser = before.isUserField == now.isUserField
                    require(
                        sameType && samePolicy && sameUser,
                    ) { "The field type or policy changed. Reload the editor." }
                    require(MetadataMapper.field(now, null).editable) { "Unsupported field type or policy." }
                }
                when (edit) {
                    is MetadataEdit.Own -> {
                        saveOwn(edit)
                    }

                    is MetadataEdit.Scalar -> {
                        requireFirstWrite(edit.id, edit.value != null)
                        val d = descriptor(edit.id)
                        require(
                            !d.isUserField && d.field.componentId.toInt() in 0xC000..0xFEFF,
                        ) { "Use My fields for user values." }
                        val shape = MetadataMapper.shape(d)
                        require(
                            shape == FieldShape.STRING || shape == FieldShape.BYTES,
                        ) { "Use an entry delta for a collection." }
                        val operation =
                            edit.value?.let { ComponentMutation.Replace(MetadataMapper.value(shape, it)) }
                                ?: ComponentMutation.Remove
                        write { conversation.updateMetadataField(d.field, operation) }
                    }

                    is MetadataEdit.Entry -> {
                        saveEntry(edit)
                    }

                    MetadataEdit.Refresh -> {
                        Unit
                    }
                }
                reload()
            }
        }
    }

    private fun descriptor(id: FieldUiId) = descriptors.getValue(id.componentId)

    private suspend fun saveOwn(edit: MetadataEdit.Own) {
        val own =
            mutableState.value.fields
                .filter {
                    it.shape == FieldShape.USER_STRING ||
                        it.shape == FieldShape.USER_BYTES
                }.associateBy { it.id }
        val updates =
            edit.values.mapNotNull { (id, text) ->
                val d = descriptor(id)
                require(d.isUserField) { "The field is not a user field." }
                val loaded = own.getValue(id)
                val value = text?.let { MetadataMapper.value(loaded.shape, it) }
                val canonical = value?.let(MetadataMapper::scalar)
                if (loaded.present == (value != null) &&
                    (value == null || loaded.scalar == canonical)
                ) {
                    return@mapNotNull null
                }
                // Include former members' entries when checking the complete map size.
                val snapshot = read { conversation.metadataValue(d.field) } as? MetadataValue.Map
                requireFirstWrite(id, value != null)
                require(!loaded.immutable || snapshot == null) { "This immutable component is already set." }
                val next =
                    snapshot
                        ?.v1
                        .orEmpty()
                        .filterNot { (it.key as? FieldKey.InboxId)?.v1 == ownInboxId }
                        .map {
                            FieldEntry(MetadataMapper.key(it.key), MetadataMapper.scalar(it.value))
                        }.toMutableList()
                if (canonical != null) next += FieldEntry(ownInboxId, canonical)
                MetadataMapper.collectionSize(loaded.shape, next)
                UserFieldUpdate(d.field, value)
            }
        if (updates.isNotEmpty()) write { conversation.updateUserData(updates) }
    }

    private fun requireFirstWrite(
        id: FieldUiId,
        insert: Boolean,
    ) {
        val field = mutableState.value.fields.single { it.id == id }
        require(!field.immutable || (!field.componentPresent && insert)) {
            "An immutable field permits only its first value."
        }
    }

    private suspend fun saveEntry(edit: MetadataEdit.Entry) {
        requireFirstWrite(edit.id, edit.action == EntryAction.INSERT)
        val d = descriptor(edit.id)
        require(!d.isUserField && d.field.componentId.toInt() in 0xC000..0xFEFF) { "Use My fields for user values." }
        val shape = MetadataMapper.shape(d)
        val key = MetadataMapper.entryKey(shape, edit.key)
        val canonical = MetadataMapper.key(key)
        val current =
            mutableState.value.fields
                .single { it.id == edit.id }
                .entries
        val exists = current.any { it.key == canonical }
        require(if (edit.action == EntryAction.INSERT) !exists else exists) { "The entry changed. Reload the editor." }
        val next = current.filterNot { it.key == canonical }.toMutableList()
        val value =
            if (shape == FieldShape.BYTE_MAP &&
                edit.action != EntryAction.DELETE
            ) {
                MetadataMapper.value(shape, edit.value)
            } else {
                null
            }
        if (edit.action != EntryAction.DELETE) next += FieldEntry(canonical, value?.let(MetadataMapper::scalar) ?: "")
        MetadataMapper.collectionSize(shape, next)
        val operation =
            if (shape == FieldShape.BYTE_MAP) {
                ComponentMutation.MapDelta(
                    listOf(
                        when (edit.action) {
                            EntryAction.INSERT -> MapMutation.Insert(key, checkNotNull(value))
                            EntryAction.UPDATE -> MapMutation.Update(key, checkNotNull(value))
                            EntryAction.DELETE -> MapMutation.Delete(key)
                        },
                    ),
                )
            } else {
                require(edit.action != EntryAction.UPDATE) { "A set supports Add and Delete." }
                ComponentMutation.SetDelta(
                    listOf(
                        if (edit.action ==
                            EntryAction.INSERT
                        ) {
                            SetMutation.Insert(key)
                        } else {
                            SetMutation.Delete(key)
                        },
                    ),
                )
            }
        write { conversation.updateMetadataField(d.field, operation) }
    }

    private suspend fun reload() {
        if (!isCurrent()) return
        val loaded = read { conversation.metadataFields() }.associateBy { it.field.componentId }
        val custom = loaded.values.filter { !it.isUserField && it.field.componentId.toInt() in 0xC000..0xFEFF }
        val users = loaded.values.filter { it.isUserField }
        val supported = custom.filter { MetadataMapper.field(it, null).editable }
        val supportedUsers =
            users.filter {
                MetadataMapper.field(it, null).editable &&
                    MetadataMapper.shape(it) in listOf(FieldShape.USER_STRING, FieldShape.USER_BYTES)
            }
        val values =
            if (supported.isEmpty()) {
                emptyMap()
            } else {
                read { conversation.metadataValues(supported.map { it.field }) }.associate {
                    it.field.componentId to
                        it.value
                }
            }
        val immutableUsers = supportedUsers.filter { MetadataMapper.field(it, null).immutable }
        val immutableValues =
            if (immutableUsers.isEmpty()) {
                emptyMap()
            } else {
                read { conversation.metadataValues(immutableUsers.map { it.field }) }
                    .associate { it.field.componentId to it.value }
            }
        val profiles =
            if (supportedUsers.isEmpty()) {
                emptyMap()
            } else {
                read { conversation.userData(supportedUsers.map { it.field }, null) }
            }
        val own = profiles[ownInboxId].orEmpty().associate { it.field.componentId to it.value }
        val fields =
            custom.map { MetadataMapper.field(it, values[it.field.componentId]) } +
                users.map {
                    val present =
                        if (MetadataMapper.field(it, null).immutable) {
                            immutableValues[it.field.componentId] != null
                        } else {
                            own[it.field.componentId] != null
                        }
                    MetadataMapper.own(it, own[it.field.componentId], present)
                }
        val members =
            profiles.filterKeys { it != ownInboxId }.map { (inbox, entries) ->
                val byId = entries.associate { it.field.componentId to it.value }
                MemberFields(
                    inbox,
                    supportedUsers.map { MetadataMapper.own(it, byId[it.field.componentId]).copy(editable = false) },
                )
            }
        val group = conversation is Conversation.Group
        val missing =
            offered
                .filter {
                    (if (group) it.inGroups else it.inDms) && it.componentId !in loaded
                }.map { it.name }
        if (isCurrent()) {
            descriptors = loaded
            mutableState.value =
                MetadataEditorState(
                    fields.sortedBy { it.id.componentId },
                    members,
                    missing,
                    busy = mutableState.value.busy,
                )
        }
    }

    private suspend fun perform(block: suspend () -> Unit) {
        if (!isCurrent()) return
        mutableState.value = mutableState.value.copy(busy = true, error = null)
        failure = null
        try {
            block()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            if (isCurrent()) {
                failure = e
                try {
                    reload()
                } catch (refresh: Exception) {
                    if (refresh is CancellationException) throw refresh
                    e.addSuppressed(refresh)
                }
                if (isCurrent()) {
                    mutableState.value =
                        mutableState.value.copy(error = "${e.javaClass.simpleName}: ${e.message ?: e}")
                }
            }
        } finally {
            if (isCurrent()) mutableState.value = mutableState.value.copy(busy = false)
        }
    }
}

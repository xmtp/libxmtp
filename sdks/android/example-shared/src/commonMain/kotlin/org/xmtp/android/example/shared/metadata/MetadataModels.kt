package org.xmtp.android.example.shared.metadata

data class FieldUiId(
    val componentId: UShort,
)

enum class FieldShape { STRING, BYTES, BYTE_MAP, BYTE_SET, INBOX_SET, USER_STRING, USER_BYTES, UNSUPPORTED }

data class FieldEntry(
    val key: String,
    val value: String = "",
)

data class FieldUi(
    val id: FieldUiId,
    val label: String,
    val shape: FieldShape,
    val policy: String,
    val present: Boolean,
    val scalar: String = "",
    val entries: List<FieldEntry> = emptyList(),
    val editable: Boolean = true,
    val unsupportedTag: Int? = null,
    val userField: Boolean = false,
    val immutable: Boolean = false,
    val componentPresent: Boolean = present,
) {
    val canWrite: Boolean get() = editable && (!immutable || !componentPresent)
}

data class MemberFields(
    val inboxId: String,
    val values: List<FieldUi>,
)

data class MetadataEditorState(
    val fields: List<FieldUi> = emptyList(),
    val members: List<MemberFields> = emptyList(),
    val missing: List<String> = emptyList(),
    val busy: Boolean = false,
    val error: String? = null,
)

enum class EntryAction { INSERT, UPDATE, DELETE }

sealed interface MetadataEdit {
    data class Scalar(
        val id: FieldUiId,
        val value: String?,
    ) : MetadataEdit

    data class Entry(
        val id: FieldUiId,
        val action: EntryAction,
        val key: String,
        val value: String = "",
    ) : MetadataEdit

    /** Values changed from the displayed baseline. */
    data class Own(
        val values: Map<FieldUiId, String?>,
    ) : MetadataEdit

    data object Refresh : MetadataEdit
}

/** Keep unsaved own values while fresh data changes untouched fields. */
data class OwnFieldDraft(
    private val baseline: Map<FieldUiId, String?> = emptyMap(),
    val values: Map<FieldUiId, String?> = emptyMap(),
) {
    fun change(
        id: FieldUiId,
        value: String?,
    ) = copy(values = values + (id to value))

    fun edit() = MetadataEdit.Own(values.filter { (id, value) -> value != baseline[id] })

    fun merge(fields: List<FieldUi>): OwnFieldDraft {
        val fresh =
            fields.filter { it.userField && it.canWrite }.associate {
                it.id to if (it.present) it.scalar else null
            }
        val nextBaseline = mutableMapOf<FieldUiId, String?>()
        val nextValues = mutableMapOf<FieldUiId, String?>()
        for ((id, committed) in fresh) {
            val dirty = id in baseline && values[id] != baseline[id]
            if (dirty && values[id] != committed) {
                nextBaseline[id] = baseline[id]
                nextValues[id] = values[id]
            } else {
                nextBaseline[id] = committed
                nextValues[id] = committed
            }
        }
        return OwnFieldDraft(nextBaseline, nextValues)
    }
}

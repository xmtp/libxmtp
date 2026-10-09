package org.xmtp.android.example.shared.metadata

data class FieldUiId(val componentId: UShort)
enum class FieldShape { STRING, BYTES, BYTE_MAP, BYTE_SET, INBOX_SET, USER_STRING, USER_BYTES, UNSUPPORTED }
data class FieldEntry(val key: String, val value: String = "")
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
)
data class MemberFields(val inboxId: String, val values: List<FieldUi>)
data class MetadataEditorState(
    val fields: List<FieldUi> = emptyList(),
    val members: List<MemberFields> = emptyList(),
    val missing: List<String> = emptyList(),
    val busy: Boolean = false,
    val error: String? = null,
)
enum class EntryAction { INSERT, UPDATE, DELETE }
sealed interface MetadataEdit {
    data class Scalar(val id: FieldUiId, val value: String?) : MetadataEdit
    data class Entry(val id: FieldUiId, val action: EntryAction, val key: String, val value: String = "") : MetadataEdit
    data class Own(val values: Map<FieldUiId, String?>) : MetadataEdit
    data object Refresh : MetadataEdit
}

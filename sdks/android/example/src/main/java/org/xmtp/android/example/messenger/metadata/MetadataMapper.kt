package org.xmtp.android.example.messenger.metadata

import org.xmtp.android.example.shared.metadata.*
import uniffi.xmtp_sdk.*

object MetadataMapper {
    fun shape(d: MetadataFieldDescriptor): FieldShape =
        when (val type = d.componentType) {
            MetadataComponentType.String -> {
                if (d.isUserField) FieldShape.UNSUPPORTED else FieldShape.STRING
            }

            MetadataComponentType.Bytes -> {
                if (d.isUserField) FieldShape.UNSUPPORTED else FieldShape.BYTES
            }

            is MetadataComponentType.Map -> {
                when {
                    type.keyType == MetadataKeyType.INBOX_ID && d.isUserField -> {
                        when (type.valueType) {
                            MetadataScalarType.STRING -> FieldShape.USER_STRING
                            MetadataScalarType.BYTES -> FieldShape.USER_BYTES
                        }
                    }

                    type.keyType == MetadataKeyType.BYTES &&
                        type.valueType == MetadataScalarType.BYTES && !d.isUserField -> {
                        FieldShape.BYTE_MAP
                    }

                    else -> {
                        FieldShape.UNSUPPORTED
                    }
                }
            }

            is MetadataComponentType.Set -> {
                if (d.isUserField) {
                    FieldShape.UNSUPPORTED
                } else {
                    when (type.keyType) {
                        MetadataKeyType.BYTES -> FieldShape.BYTE_SET
                        MetadataKeyType.INBOX_ID -> FieldShape.INBOX_SET
                    }
                }
            }

            is MetadataComponentType.Unknown -> {
                FieldShape.UNSUPPORTED
            }
        }

    fun policy(p: MetadataPolicy): String =
        when (p) {
            is MetadataPolicy.Base -> {
                when (val base = p.v1) {
                    MetadataBasePolicy.Allow -> "All members"
                    MetadataBasePolicy.Deny -> "Denied"
                    MetadataBasePolicy.AllowIfAdmin -> "Admins"
                    MetadataBasePolicy.AllowIfSuperAdmin -> "Super admins"
                    MetadataBasePolicy.AllowIfSelfOrNonMember -> "Own entry"
                    is MetadataBasePolicy.Unknown -> "Unsupported policy ${base.tag}"
                }
            }

            is MetadataPolicy.And -> {
                p.v1.joinToString(" and ", "(", ")", transform = ::policy)
            }

            is MetadataPolicy.Any -> {
                p.v1.joinToString(" or ", "(", ")", transform = ::policy)
            }
        }

    private fun known(p: MetadataPolicy): Boolean =
        when (p) {
            is MetadataPolicy.Base -> p.v1 !is MetadataBasePolicy.Unknown
            is MetadataPolicy.And -> p.v1.all(::known)
            is MetadataPolicy.Any -> p.v1.all(::known)
        }

    fun field(
        d: MetadataFieldDescriptor,
        value: MetadataValue?,
    ): FieldUi {
        val shape = shape(d)
        return FieldUi(
            FieldUiId(d.field.componentId),
            d.field.name ?: "Component ${d.field.componentId}",
            shape,
            "Insert: ${policy(
                d.permissions.insert,
            )}; Update: ${policy(d.permissions.update)}; Delete: ${policy(d.permissions.delete)}",
            value != null,
            scalar = (value as? MetadataValue.Scalar)?.v1?.let(::scalar) ?: "",
            entries =
                when (value) {
                    is MetadataValue.Map -> value.v1.map { FieldEntry(key(it.key), scalar(it.value)) }
                    is MetadataValue.Set -> value.v1.map { FieldEntry(key(it)) }
                    else -> emptyList()
                },
            editable =
                shape != FieldShape.UNSUPPORTED &&
                    listOf(d.permissions.insert, d.permissions.update, d.permissions.delete).all(::known),
            unsupportedTag = (d.componentType as? MetadataComponentType.Unknown)?.tag,
            userField = d.isUserField,
            immutable = d.field.componentId.toInt() in 0xFD00..0xFEFF,
        )
    }

    fun own(
        d: MetadataFieldDescriptor,
        value: FieldValue?,
        componentPresent: Boolean = value != null,
    ): FieldUi =
        field(d, null).copy(
            present = value != null,
            componentPresent = componentPresent,
            scalar =
                value?.let(::scalar) ?: "",
        )

    fun scalar(value: FieldValue): String =
        when (value) {
            is FieldValue.String -> value.v1
            is FieldValue.Bytes -> hex(value.v1)
        }

    fun key(value: FieldKey): String =
        when (value) {
            is FieldKey.Bytes -> hex(value.v1)
            is FieldKey.InboxId -> value.v1
        }

    fun hex(value: ByteArray): String = value.joinToString("") { "%02x".format(it.toInt() and 255) }

    fun bytes(text: String): ByteArray {
        require(
            text.length % 2 == 0 &&
                text.all {
                    it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F'
                },
        ) { "Use an even number of hex digits." }
        require(text.length <= 8192 * 2) { "A value or key must be at most 8192 bytes." }
        return ByteArray(text.length / 2) { text.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    }

    fun inbox(text: String): String {
        require(
            text.length == 64 && text.all { it in '0'..'9' || it in 'a'..'f' },
        ) { "Use a lowercase 64-digit inbox ID." }
        return text
    }

    fun value(
        shape: FieldShape,
        text: String,
    ): FieldValue =
        when (shape) {
            FieldShape.STRING, FieldShape.USER_STRING -> {
                require(text.toByteArray(Charsets.UTF_8).size <= 8192) { "A value must be at most 8192 bytes." }
                FieldValue.String(text)
            }

            FieldShape.BYTES, FieldShape.BYTE_MAP, FieldShape.USER_BYTES -> {
                FieldValue.Bytes(bytes(text))
            }

            else -> {
                error("Unsupported scalar type.")
            }
        }

    fun entryKey(
        shape: FieldShape,
        text: String,
    ): FieldKey =
        when (shape) {
            FieldShape.BYTE_MAP, FieldShape.BYTE_SET -> FieldKey.Bytes(bytes(text))
            FieldShape.INBOX_SET -> FieldKey.InboxId(inbox(text))
            else -> error("Unsupported key type.")
        }

    /** Check the resulting TLS collection size. The SDK checks the final commit. */
    fun collectionSize(
        shape: FieldShape,
        entries: List<FieldEntry>,
    ) {
        fun prefix(size: Int) =
            if (size < 64) {
                1
            } else if (size < 16384) {
                2
            } else {
                4
            }
        var payload = 0
        for (entry in entries) {
            val keySize =
                if (shape in
                    listOf(FieldShape.INBOX_SET, FieldShape.USER_STRING, FieldShape.USER_BYTES)
                ) {
                    inbox(entry.key)
                    33
                } else {
                    bytes(entry.key).size.let {
                        it +
                            prefix(it)
                    }
                }
            val valueSize =
                when (shape) {
                    FieldShape.BYTE_MAP, FieldShape.USER_BYTES -> {
                        bytes(entry.value).size.let { it + prefix(it) }
                    }

                    FieldShape.USER_STRING -> {
                        entry.value.toByteArray(Charsets.UTF_8).size.let {
                            require(it <= 8192)
                            it +
                                prefix(it)
                        }
                    }

                    else -> {
                        0
                    }
                }
            payload += keySize + valueSize
            require(payload <= 65536) { "A collection must be at most 65536 serialized bytes." }
        }
        require(payload + prefix(payload) <= 65536) { "A collection must be at most 65536 serialized bytes." }
    }
}

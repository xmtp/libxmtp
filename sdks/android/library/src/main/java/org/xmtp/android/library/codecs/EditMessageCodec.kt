package org.xmtp.android.library.codecs

data class EditMessageRequest(
    val messageId: String,
    val editedContent: EncodedContent?,
)

val ContentTypeEditMessageRequest =
    ContentTypeIdBuilder.builderFromAuthorityId(
        "xmtp.org",
        "editMessage",
        versionMajor = 1,
        versionMinor = 0,
    )

data class EditMessageCodec(
    override var contentType: ContentTypeId = ContentTypeEditMessageRequest,
) : ContentCodec<EditMessageRequest> {
    override fun encode(content: EditMessageRequest): EncodedContent {
        val ffiEditedContent =
            content.editedContent?.let { encoded ->
                uniffi.xmtpv3.FfiEncodedContent(
                    typeId =
                        encoded.type?.let { type ->
                            uniffi.xmtpv3.FfiContentTypeId(
                                authorityId = type.authorityId,
                                typeId = type.typeId,
                                versionMajor = type.versionMajor.toUInt(),
                                versionMinor = type.versionMinor.toUInt(),
                            )
                        },
                    parameters = encoded.parametersMap,
                    fallback = encoded.fallback.ifEmpty { null },
                    compression = encoded.compression?.let { it.number },
                    content = encoded.content.toByteArray(),
                )
            }

        val ffi =
            uniffi.xmtpv3.FfiEditMessage(
                messageId = content.messageId,
                editedContent = ffiEditedContent,
            )

        return EncodedContent.parseFrom(
            uniffi.xmtpv3.encodeEditMessage(ffi),
        )
    }

    override fun decode(content: EncodedContent): EditMessageRequest {
        val decoded = uniffi.xmtpv3.decodeEditMessage(content.toByteArray())

        val editedContent =
            decoded.editedContent?.let { ffiContent ->
                EncodedContent
                    .newBuilder()
                    .apply {
                        ffiContent.typeId?.let { type ->
                            setType(
                                ContentTypeId
                                    .newBuilder()
                                    .setAuthorityId(type.authorityId)
                                    .setTypeId(type.typeId)
                                    .setVersionMajor(type.versionMajor.toInt())
                                    .setVersionMinor(type.versionMinor.toInt())
                                    .build(),
                            )
                        }
                        putAllParameters(ffiContent.parameters)
                        ffiContent.fallback?.let { setFallback(it) }
                        ffiContent.compression?.let { setCompression(Compression.forNumber(it)) }
                        setContent(
                            com.google.protobuf.ByteString
                                .copyFrom(ffiContent.content),
                        )
                    }.build()
            }

        return EditMessageRequest(
            messageId = decoded.messageId,
            editedContent = editedContent,
        )
    }

    override fun fallback(content: EditMessageRequest): String? = null

    override fun shouldPush(content: EditMessageRequest): Boolean = false
}

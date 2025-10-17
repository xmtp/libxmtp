package org.xmtp.android.library.codecs

import uniffi.xmtpv3.FfiContentTypeId

typealias ContentTypeId = org.xmtp.proto.message.contents.Content.ContentTypeId

class ContentTypeIdBuilder {
    companion object {
        fun builderFromAuthorityId(
            authorityId: String,
            typeId: String,
            versionMajor: Int,
            versionMinor: Int,
        ): ContentTypeId =
            ContentTypeId
                .newBuilder()
                .also {
                    it.authorityId = authorityId
                    it.typeId = typeId
                    it.versionMajor = versionMajor
                    it.versionMinor = versionMinor
                }.build()

        fun fromFfi(ffiContentTypeId: FfiContentTypeId): ContentTypeId =
            ContentTypeId
                .newBuilder()
                .also {
                    it.authorityId = ffiContentTypeId.authorityId
                    it.typeId = ffiContentTypeId.typeId
                    it.versionMajor = ffiContentTypeId.versionMajor.toInt()
                    it.versionMinor = ffiContentTypeId.versionMinor.toInt()
                }.build()
    }
}

val ContentTypeId.id: String
    get() = "$authorityId:$typeId:$versionMajor.$versionMinor"

val ContentTypeId.description: String
    get() = "$authorityId/$typeId:$versionMajor.$versionMinor"

package org.xmtp.android.library.codecs

typealias ContentTypeId = org.xmtp.proto.message.contents.Content.ContentTypeId

class ContentTypeIdBuilder {
    companion object {
        fun builderFromAuthorityId(
            authorityId: String,
            typeId: String,
            versionMajor: Int,
            versionMinor: Int
        ): ContentTypeId {
            return ContentTypeId.newBuilder().also {
                it.authorityId = authorityId
                it.typeId = typeId
                it.versionMajor = versionMajor
                it.versionMinor = versionMinor
            }.build()
        }
    }
}

val ContentTypeId.id: String
    get() = "$authorityId:$typeId"

val ContentTypeId.description: String
    get() = "$authorityId/$typeId:$versionMajor.$versionMinor"

package org.xmtp.android.library.codecs

import org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupUpdated

typealias GroupUpdated = GroupUpdated

val ContentTypeGroupUpdated =
    ContentTypeIdBuilder.builderFromAuthorityId(
        "xmtp.org",
        "group_updated",
        versionMajor = 1,
        versionMinor = 0,
    )

data class GroupUpdatedCodec(
    override var contentType: ContentTypeId = ContentTypeGroupUpdated,
) : ContentCodec<GroupUpdated> {
    override fun encode(content: GroupUpdated): EncodedContent =
        EncodedContent
            .newBuilder()
            .also {
                it.type = ContentTypeGroupUpdated
                it.content = content.toByteString()
            }.build()

    override fun decode(content: EncodedContent): GroupUpdated = GroupUpdated.parseFrom(content.content)

    override fun fallback(content: GroupUpdated): String? = null

    override fun shouldPush(content: GroupUpdated): Boolean = false
}

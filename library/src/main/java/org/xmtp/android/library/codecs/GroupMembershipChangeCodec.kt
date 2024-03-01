package uniffi.xmtpv3.org.xmtp.android.library.codecs

import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.EncodedContent

typealias GroupMembershipChanges = org.xmtp.proto.mls.message.contents.TranscriptMessages.GroupMembershipChanges

val ContentTypeGroupMembershipChange = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "group_membership_change",
    versionMajor = 1,
    versionMinor = 0,
)

data class GroupMembershipChangeCodec(override var contentType: ContentTypeId = ContentTypeGroupMembershipChange) :
    ContentCodec<GroupMembershipChanges> {

    override fun encode(content: GroupMembershipChanges): EncodedContent {
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeGroupMembershipChange
            it.content = content.toByteString()
        }.build()
    }

    override fun decode(content: EncodedContent): GroupMembershipChanges {
        return GroupMembershipChanges.parseFrom(content.content)
    }

    override fun fallback(content: GroupMembershipChanges): String? {
        return null
    }

    override fun shouldPush(content: GroupMembershipChanges): Boolean = false
}

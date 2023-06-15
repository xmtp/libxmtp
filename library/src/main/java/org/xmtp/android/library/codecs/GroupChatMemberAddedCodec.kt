package uniffi.xmtp_dh.org.xmtp.android.library.codecs

import com.google.gson.GsonBuilder
import com.google.protobuf.kotlin.toByteStringUtf8
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.EncodedContent

val ContentTypeGroupChatMemberAdded = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "groupChatMemberAdded",
    versionMajor = 1,
    versionMinor = 0
)

// The address of the member being added
data class GroupChatMemberAdded(var member: String)

data class GroupChatMemberAddedCodec(override var contentType: ContentTypeId = ContentTypeGroupChatMemberAdded) :
    ContentCodec<GroupChatMemberAdded> {

    override fun encode(content: GroupChatMemberAdded): EncodedContent {
        val gson = GsonBuilder().create()
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeGroupChatMemberAdded
            it.content = gson.toJson(content).toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): GroupChatMemberAdded {
        val gson = GsonBuilder().create()
        return gson.fromJson(content.content.toStringUtf8(), GroupChatMemberAdded::class.java)
    }
}

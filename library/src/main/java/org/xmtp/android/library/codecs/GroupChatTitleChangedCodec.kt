package uniffi.xmtp_dh.org.xmtp.android.library.codecs

import com.google.gson.GsonBuilder
import com.google.protobuf.kotlin.toByteStringUtf8
import org.xmtp.android.library.codecs.ContentCodec
import org.xmtp.android.library.codecs.ContentTypeId
import org.xmtp.android.library.codecs.ContentTypeIdBuilder
import org.xmtp.android.library.codecs.EncodedContent

val ContentTypeGroupTitleChangedAdded = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "groupChatTitleChanged",
    versionMajor = 1,
    versionMinor = 0
)

// The new title
data class GroupChatTitleChanged(var newTitle: String)

data class GroupChatTitleChangedCodec(override var contentType: ContentTypeId = ContentTypeGroupTitleChangedAdded) :
    ContentCodec<GroupChatTitleChanged> {

    override fun encode(content: GroupChatTitleChanged): EncodedContent {
        val gson = GsonBuilder().create()
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeGroupTitleChangedAdded
            it.content = gson.toJson(content).toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): GroupChatTitleChanged {
        val gson = GsonBuilder().create()
        return gson.fromJson(content.content.toStringUtf8(), GroupChatTitleChanged::class.java)
    }
}

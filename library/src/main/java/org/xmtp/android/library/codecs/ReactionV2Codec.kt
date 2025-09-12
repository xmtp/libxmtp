package org.xmtp.android.library.codecs

import uniffi.xmtpv3.FfiReactionPayload
import uniffi.xmtpv3.FfiReactionAction
import uniffi.xmtpv3.decodeReaction
import uniffi.xmtpv3.encodeReaction

val ContentTypeReactionV2 = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "reaction",
    versionMajor = 2,
    versionMinor = 0,
)

data class ReactionV2Codec(override var contentType: ContentTypeId = ContentTypeReactionV2) :
    ContentCodec<FfiReactionPayload> {

    override fun encode(content: FfiReactionPayload): EncodedContent {
        return EncodedContent.parseFrom(encodeReaction(content))
    }

    override fun decode(content: EncodedContent): FfiReactionPayload {
        return decodeReaction(content.toByteArray())
    }

    override fun fallback(content: FfiReactionPayload): String? {
        return when (content.action) {
            FfiReactionAction.ADDED -> "Reacted \"${content.content}\" to an earlier message"
            FfiReactionAction.REMOVED -> "Removed \"${content.content}\" from an earlier message"
            else -> null
        }
    }

    override fun shouldPush(content: FfiReactionPayload): Boolean = when (content.action) {
        FfiReactionAction.ADDED -> true
        else -> false
    }
}

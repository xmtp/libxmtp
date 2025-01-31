package org.xmtp.android.library.codecs

import uniffi.xmtpv3.FfiReaction
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
    ContentCodec<FfiReaction> {

    override fun encode(content: FfiReaction): EncodedContent {
        return EncodedContent.parseFrom(encodeReaction(content))
    }

    override fun decode(content: EncodedContent): FfiReaction {
        return decodeReaction(content.toByteArray())
    }

    override fun fallback(content: FfiReaction): String? {
        return when (content.action) {
            FfiReactionAction.ADDED -> "Reacted “${content.content}” to an earlier message"
            FfiReactionAction.REMOVED -> "Removed “${content.content}” from an earlier message"
            else -> null
        }
    }

    override fun shouldPush(content: FfiReaction): Boolean = when (content.action) {
        FfiReactionAction.ADDED -> true
        else -> false
    }
}

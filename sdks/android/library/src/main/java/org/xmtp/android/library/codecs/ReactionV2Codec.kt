package org.xmtp.android.library.codecs

import uniffi.xmtpv3.FfiReactionAction
import uniffi.xmtpv3.FfiReactionPayload
import uniffi.xmtpv3.FfiReactionSchema
import uniffi.xmtpv3.decodeReaction
import uniffi.xmtpv3.encodeReaction

val ContentTypeReactionV2 =
    ContentTypeIdBuilder.builderFromAuthorityId(
        "xmtp.org",
        "reaction",
        versionMajor = 2,
        versionMinor = 0,
    )

private fun ReactionAction.toFfi(): FfiReactionAction =
    when (this) {
        ReactionAction.Added -> FfiReactionAction.ADDED
        ReactionAction.Removed -> FfiReactionAction.REMOVED
        ReactionAction.Unknown -> FfiReactionAction.UNKNOWN
    }

private fun FfiReactionAction.toReactionAction(): ReactionAction =
    when (this) {
        FfiReactionAction.ADDED -> ReactionAction.Added
        FfiReactionAction.REMOVED -> ReactionAction.Removed
        FfiReactionAction.UNKNOWN -> ReactionAction.Unknown
    }

private fun ReactionSchema.toFfi(): FfiReactionSchema =
    when (this) {
        ReactionSchema.Unicode -> FfiReactionSchema.UNICODE
        ReactionSchema.Shortcode -> FfiReactionSchema.SHORTCODE
        ReactionSchema.Custom -> FfiReactionSchema.CUSTOM
        ReactionSchema.Unknown -> FfiReactionSchema.UNKNOWN
    }

private fun FfiReactionSchema.toReactionSchema(): ReactionSchema =
    when (this) {
        FfiReactionSchema.UNICODE -> ReactionSchema.Unicode
        FfiReactionSchema.SHORTCODE -> ReactionSchema.Shortcode
        FfiReactionSchema.CUSTOM -> ReactionSchema.Custom
        FfiReactionSchema.UNKNOWN -> ReactionSchema.Unknown
    }

private fun Reaction.toFfiPayload(): FfiReactionPayload =
    FfiReactionPayload(
        reference = reference,
        referenceInboxId = referenceInboxId.orEmpty(),
        action = action.toFfi(),
        content = content,
        schema = schema.toFfi(),
    )

private fun FfiReactionPayload.toReaction(): Reaction =
    Reaction(
        reference = reference,
        action = action.toReactionAction(),
        content = content,
        schema = schema.toReactionSchema(),
        referenceInboxId = referenceInboxId.ifEmpty { null },
    )

data class ReactionV2Codec(
    override var contentType: ContentTypeId = ContentTypeReactionV2,
) : ContentCodec<Reaction> {
    override fun encode(content: Reaction): EncodedContent =
        EncodedContent.parseFrom(encodeReaction(content.toFfiPayload()))

    override fun decode(content: EncodedContent): Reaction = decodeReaction(content.toByteArray()).toReaction()

    override fun fallback(content: Reaction): String? =
        when (content.action) {
            ReactionAction.Added -> "Reacted \"${content.content}\" to an earlier message"
            ReactionAction.Removed -> "Removed \"${content.content}\" from an earlier message"
            else -> null
        }

    override fun shouldPush(content: Reaction): Boolean = content.action == ReactionAction.Added
}

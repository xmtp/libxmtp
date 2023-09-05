package org.xmtp.android.library.codecs

import com.google.gson.GsonBuilder
import com.google.protobuf.kotlin.toByteStringUtf8

val ContentTypeReaction = ContentTypeIdBuilder.builderFromAuthorityId(
    "xmtp.org",
    "reaction",
    versionMajor = 1,
    versionMinor = 0
)

data class Reaction(
    val reference: String,
    val action: ReactionAction,
    val content: String,
    val schema: ReactionSchema,
)

enum class ReactionAction {
    added, removed
}

enum class ReactionSchema {
    unicode, shortcode, custom
}

data class ReactionCodec(override var contentType: ContentTypeId = ContentTypeReaction) :
    ContentCodec<Reaction> {

    override fun encode(content: Reaction): EncodedContent {
        val gson = GsonBuilder().create()
        return EncodedContent.newBuilder().also {
            it.type = ContentTypeReaction
            it.content = gson.toJson(content).toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): Reaction {
        val text = content.content.toStringUtf8()

        // First try to decode it in the canonical form.
        try {
            return GsonBuilder().create().fromJson(text, Reaction::class.java)
        } catch (ignore: Exception) {
        }

        // If that fails, try to decode it in the legacy form.
        return Reaction(
            reference = content.parametersMap["reference"] ?: "",
            action = content.parametersMap["action"]?.let { ReactionAction.valueOf(it) }!!,
            schema = content.parametersMap["schema"]?.let { ReactionSchema.valueOf(it) }!!,
            content = text,
        )
    }
}

package org.xmtp.android.library.codecs

import com.google.gson.GsonBuilder
import com.google.gson.JsonDeserializationContext
import com.google.gson.JsonDeserializer
import com.google.gson.JsonElement
import com.google.gson.JsonObject
import com.google.gson.JsonSerializationContext
import com.google.gson.JsonSerializer
import com.google.protobuf.kotlin.toByteStringUtf8
import java.lang.reflect.Type

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

sealed class ReactionAction {
    object Removed : ReactionAction()
    object Added : ReactionAction()
    object Unknown : ReactionAction()
}

sealed class ReactionSchema {
    object Unicode : ReactionSchema()
    object Shortcode : ReactionSchema()
    object Custom : ReactionSchema()
    object Unknown : ReactionSchema()
}

fun getReactionSchema(schema: String): ReactionSchema {
    return when (schema) {
        "unicode" -> ReactionSchema.Unicode
        "shortcode" -> ReactionSchema.Shortcode
        "custom" -> ReactionSchema.Custom
        else -> ReactionSchema.Unknown
    }
}

fun getReactionAction(action: String): ReactionAction {
    return when (action) {
        "removed" -> ReactionAction.Removed
        "added" -> ReactionAction.Added
        else -> ReactionAction.Unknown
    }
}

data class ReactionCodec(override var contentType: ContentTypeId = ContentTypeReaction) :
    ContentCodec<Reaction> {

    override fun encode(content: Reaction): EncodedContent {
        val gson = GsonBuilder()
            .registerTypeAdapter(Reaction::class.java, ReactionSerializer())
            .create()

        return EncodedContent.newBuilder().also {
            it.type = ContentTypeReaction
            it.content = gson.toJson(content).toByteStringUtf8()
        }.build()
    }

    override fun decode(content: EncodedContent): Reaction {
        val json = content.content.toStringUtf8()

        val gson = GsonBuilder()
            .registerTypeAdapter(Reaction::class.java, ReactionDeserializer())
            .create()
        try {
            return gson.fromJson(json, Reaction::class.java)
        } catch (ignore: Exception) {
        }

        // If that fails, try to decode it in the legacy form.
        return Reaction(
            reference = content.parametersMap["reference"] ?: "",
            action = getReactionAction(content.parametersMap["action"]?.lowercase() ?: ""),
            schema = getReactionSchema(content.parametersMap["schema"]?.lowercase() ?: ""),
            content = json,
        )
    }

    override fun fallback(content: Reaction): String? {
        return when (content.action) {
            ReactionAction.Added -> "Reacted “${content.content}” to an earlier message"
            ReactionAction.Removed -> "Removed “${content.content}” from an earlier message"
            else -> null
        }
    }
}

private class ReactionSerializer : JsonSerializer<Reaction> {
    override fun serialize(
        src: Reaction,
        typeOfSrc: Type,
        context: JsonSerializationContext,
    ): JsonObject {
        val json = JsonObject()
        json.addProperty("reference", src.reference)
        json.addProperty("action", src.action.javaClass.simpleName.lowercase())
        json.addProperty("content", src.content)
        json.addProperty("schema", src.schema.javaClass.simpleName.lowercase())
        return json
    }
}

private class ReactionDeserializer : JsonDeserializer<Reaction> {
    override fun deserialize(
        json: JsonElement,
        typeOfT: Type?,
        context: JsonDeserializationContext?,
    ): Reaction {
        val jsonObject = json.asJsonObject
        val reference = jsonObject.get("reference").asString
        val actionStr = jsonObject.get("action").asString.lowercase()
        val content = jsonObject.get("content").asString
        val schemaStr = jsonObject.get("schema").asString.lowercase()

        val action = getReactionAction(actionStr)
        val schema = getReactionSchema(schemaStr)

        return Reaction(reference, action, content, schema)
    }
}

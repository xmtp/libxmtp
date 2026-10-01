package org.xmtp.benchmark

data class LiveAttachment(
    val filename: String,
    val mimeType: String,
    val bytesHex: String,
)

data class LiveReaction(
    val content: String,
    val schema: String,
    val action: String,
)

data class LiveEvent(
    val id: String,
    val kind: String,
    val text: String? = null,
    val reference: String? = null,
    val attachment: LiveAttachment? = null,
    val reaction: LiveReaction? = null,
    val eagerParentText: String? = null,
    val eagerReactions: List<LiveReaction> = emptyList(),
)

data class LiveRow(
    val key: String,
    val text: String?,
    val replyTo: String?,
    val parentText: String?,
    val attachment: LiveAttachment?,
    val reactions: List<LiveReaction>,
)

// Both SDKs produce this rich result from actual delivered values inside timing.
fun enrichLive(
    events: List<LiveEvent>,
    ids: List<String>,
): List<LiveRow> {
    val byId = mutableMapOf<String, LiveEvent>()
    for (event in events) {
        check(byId.put(event.id, event) == null) { "Duplicate live event" }
    }
    val keys = ids.mapIndexed { index, id -> id to index.toString() }.toMap()
    val reactions = ids.associateWith { mutableListOf<LiveReaction>() }
    for (event in events) {
        if (event.kind != "reaction") continue
        val target = checkNotNull(reactions[event.reference]) { "Missing live reaction target" }
        target += checkNotNull(event.reaction) { "Missing live reaction content" }
    }
    return ids.map { id ->
        val event = checkNotNull(byId[id]) { "Missing live primary content" }
        check(event.kind in listOf("text", "reply", "attachment")) { "Unsupported live primary content" }
        var parent: String? = null
        var parentText: String? = null
        if (event.kind == "reply") {
            parent = checkNotNull(keys[event.reference]) { "Missing delivered reply parent ID" }
            val original = checkNotNull(byId[event.reference]) { "Missing delivered reply parent" }
            check(original.kind == "text") { "Unexpected delivered parent content" }
            parentText = checkNotNull(original.text) { "Missing delivered parent text" }
            check(event.eagerParentText == null || event.eagerParentText == parentText) {
                "Eager reply parent differs from the delivered parent"
            }
        }
        check(event.kind == "attachment" || event.text != null) { "Missing live text or reply body" }
        check(event.kind != "attachment" || event.attachment != null) { "Missing live attachment" }
        val delivered = reactions.getValue(id)
        check(event.eagerReactions.all { it in delivered }) { "Eager reaction differs from the delivered reactions" }
        LiveRow(
            keys.getValue(id),
            if (event.kind == "attachment") null else event.text,
            parent,
            parentText,
            event.attachment,
            delivered.toList(),
        )
    }
}

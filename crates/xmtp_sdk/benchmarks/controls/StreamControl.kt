package org.xmtp.benchmark

var controlHistory: List<LiveRow> = emptyList()

fun main() {
    controlHistory =
        (0 until 10000).map { index ->
            LiveRow(
                index.toString(),
                if (index % 4 == 2) null else "cutover message %05d".format(index),
                if (index % 4 == 1) (index - 1).toString() else null,
                if (index % 4 == 1) "cutover message %05d".format(index - 1) else null,
                if (index % 4 == 2) {
                    LiveAttachment(
                        "fixture-%05d.bin".format(index),
                        "application/octet-stream",
                        (0 until 128).joinToString("") { "%02x".format((index + it) % 256) },
                    )
                } else {
                    null
                },
                if (index % 4 == 3) listOf(LiveReaction("+1", "unicode", "added")) else emptyList(),
            )
        }
    val ids = controlHistory.map { "p" + it.key }
    for (eager in listOf(false, true)) {
        val source =
            controlHistory.flatMap { row ->
                val id = "p" + row.key
                listOf(
                    LiveEvent(
                        id,
                        if (row.attachment !=
                            null
                        ) {
                            "attachment"
                        } else if (row.replyTo != null) {
                            "reply"
                        } else {
                            "text"
                        },
                        text = row.text,
                        reference = row.replyTo?.let { "p$it" },
                        attachment = row.attachment,
                        eagerParentText = if (eager) row.parentText else null,
                        eagerReactions = if (eager) row.reactions else emptyList(),
                    ),
                ) +
                    row.reactions.map { LiveEvent("r" + row.key, "reaction", reference = id, reaction = it) }
            }
        val faults =
            listOf("drop_content", "change_text", "change_reply", "change_attachment", "change_reaction") +
                (if (eager) listOf("change_eager_parent") else emptyList()) + listOf("good")
        for (fault in faults) {
            val events = source.toMutableList()
            if (fault == "drop_content") events[0] = events[0].copy(text = null)
            if (fault == "change_text") events[0] = events[0].copy(text = "corrupt live text")
            if (fault == "change_reply") events[1] = events[1].copy(text = "corrupt live reply")
            if (fault ==
                "change_attachment"
            ) {
                events[2] =
                    events[2].copy(attachment = events[2].attachment!!.copy(bytesHex = "00"))
            }
            if (fault == "change_reaction") {
                val index = events.indexOfFirst { it.kind == "reaction" }
                events[index] =
                    events[index].copy(reaction = events[index].reaction!!.copy(content = "corrupt live reaction"))
            }
            if (fault == "change_eager_parent") events[1] = events[1].copy(eagerParentText = "corrupt eager parent")
            val result =
                runCatching {
                    check(
                        enrichLive(events, ids) == controlHistory,
                    ) { "Live semantic result differs from correct history" }
                }
            println(
                "target=kotlin eager=$eager fault=$fault rejected=${result.isFailure} " +
                    "correct_history_messages=${controlHistory.size} failure=${result.exceptionOrNull()?.message}",
            )
            check((fault == "good") == result.isSuccess) { "Live control did not detect the fault" }
        }
    }
}

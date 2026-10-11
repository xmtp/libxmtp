package org.xmtp.android.example.shared.feature.chat

import org.xmtp.android.example.shared.*

object ChatFixtures {
    private val sendFailure =
        AppFailure("Message was not sent. Retry this message.", "send-unavailable", FailureRecovery.RETRY)
    val messages =
        listOf(
            MessageRow(
                "message-1",
                "Alex Chen",
                "Are you free this evening?",
                "10:41",
                "10 October 2026",
                100L,
                false,
                "Sent",
            ),
            MessageRow(
                "message-2",
                "You",
                "Yes. See you at 6.",
                "10:42",
                "10 October 2026",
                200L,
                true,
                "Sent",
                reactions = listOf(ReactionUi("👍", 1, false, listOf(ReactionIdentity("sample-alex", "Alex Chen")))),
            ),
        )
    val empty = ChatState(conversationId = "conversation-alex", title = "Alex Chen", hasOlder = false)
    val populated = empty.copy(messages = messages.asReversed())
    val loading = empty.copy(busy = true)
    val failure =
        populated.copy(
            failure = AppFailure("Cannot load messages. Try again.", "history-unavailable", FailureRecovery.RETRY),
        )
    val reply =
        populated.copy(
            composer =
                ComposerState(
                    text = "I will bring the notes.",
                    selectionStart = 23,
                    selectionEnd = 23,
                    revision = 1,
                    reply = ReplyTarget("message-1", "Alex Chen", "Are you free this evening?"),
                ),
        )
    val selected = populated.copy(overlay = MessageOverlayState("message-2"))
    val emoji = populated.copy(overlay = MessageOverlayState("message-2", MessageSurface.EMOJI, "smile"))
    val reactors = populated.copy(overlay = MessageOverlayState("message-2", MessageSurface.REACTORS, emoji = "👍"))
    val history = populated.copy(hasOlder = true, historyNotice = "More messages are available.")
    val recovery =
        populated.copy(
            hasOlderRecovery = true,
            recoveryAtNewest = false,
            recoveryNotice = "Some messages are waiting to be sent.",
        )
    val sending = populated.copy(messages = populated.messages.map { if (it.mine) it.copy(status = "Sending") else it })
    val queued = populated.copy(messages = populated.messages.map { if (it.mine) it.copy(status = "Queued") else it })
    val sent = populated
    val notSent =
        populated.copy(
            messages = populated.messages.map { if (it.mine) it.copy(status = "Not sent") else it },
        )
    val unknown =
        populated.copy(
            messages =
                populated.messages.map {
                    if (it.mine) it.copy(status = "Unknown outcome") else it
                },
            composer =
                ComposerState(
                    text = "Yes. See you at 6.",
                    revision = 1,
                    result = TextSendResult(1, "conversation-alex", 1, TextSendOutcome.Unknown("draft-1")),
                    enabled = false,
                ),
        )
    val acceptedPendingCommit =
        populated.copy(
            composer =
                ComposerState(
                    text = "Yes. See you at 6.",
                    revision = 1,
                    submittedRevision = 1,
                    result =
                        TextSendResult(
                            1,
                            "conversation-alex",
                            1,
                            TextSendOutcome.AcceptedPendingCommit("message-2", sendFailure),
                        ),
                    enabled = false,
                ),
        )
    val rejected =
        populated.copy(
            composer =
                ComposerState(
                    text = "I will bring the notes.",
                    revision = 1,
                    result = TextSendResult(2, "conversation-alex", 1, TextSendOutcome.Rejected(sendFailure)),
                ),
        )
    val longContent =
        populated
            .copy(
                title = "Alexandra Chen and the planning team",
                group = true,
                messages =
                    populated.messages.map {
                        if (it.mine) {
                            it
                        } else {
                            it.copy(
                                sender = "Alexandra Chen",
                                text =
                                    "Please bring the notes for tomorrow’s meeting. " +
                                        "We can review the full plan together before we start. " +
                                        "The meeting room is next to the main entrance.",
                            )
                        }
                    },
            )
    val request = populated.copy(unknown = true)
}

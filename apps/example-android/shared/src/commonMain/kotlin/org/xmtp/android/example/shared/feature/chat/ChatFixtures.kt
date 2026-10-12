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
    val populated = empty.copy(messages = messages)
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
    val sending = populated.copy(messages = messages.dropLast(1) + messages.last().copy(status = "Sending"))
    val queued = populated.copy(messages = messages.dropLast(1) + messages.last().copy(status = "Queued"))
    val sent = populated
    val notSent = populated.copy(messages = messages.dropLast(1) + messages.last().copy(status = "Not sent"))
    val unknown =
        populated.copy(
            messages = messages.dropLast(1) + messages.last().copy(status = "Unknown outcome"),
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
    val request = populated.copy(unknown = true)
}

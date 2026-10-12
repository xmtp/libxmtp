package org.xmtp.android.example.shared.feature.conversations

import org.xmtp.android.example.shared.AppFailure
import org.xmtp.android.example.shared.ConversationRow
import org.xmtp.android.example.shared.FailureRecovery

object ConversationsFixtures {
    val rows =
        listOf(
            ConversationRow("conversation-alex", "Alex Chen", "See you at 6.", "10:42", "2", false, 1, muted = false),
            ConversationRow(
                "conversation-team",
                "Project team",
                "Maya: The notes are ready.",
                "09:15",
                "",
                false,
                2,
                muted = true,
                group = true,
            ),
        )
    val requestRows =
        listOf(ConversationRow("conversation-request", "New contact", "Can we talk?", "Yesterday", "1", true, 3))
    val empty = ConversationsState(hasMore = false)
    val populated =
        ConversationsState(
            rows = rows,
            requests = requestRows,
            requestCount = 1,
            requestCountValid = true,
            hasMore = false,
        )
    val loading = empty.copy(busy = true)
    val failure =
        populated.copy(
            failure = AppFailure("Cannot load chats. Try again.", "chats-unavailable", FailureRecovery.RETRY),
        )
    val offline = populated.copy(connection = "Offline")
    val reconnecting = populated.copy(connection = "Reconnecting")
    val requests = populated.copy(rows = requestRows, unknownTab = true)
    val search = populated.copy(query = "Alex")
    val request = RequestState("conversation-request", "New contact", "sample-new-contact")
}

object CreateConversationFixtures {
    val empty = CreateConversationState()
    val populated =
        CreateConversationState(
            group = true,
            recipients = "sample-alex,sample-maya",
            name = "Project team",
            selectedRecipients =
                listOf(
                    RecipientUi("sample-alex", "Alex Chen", true),
                    RecipientUi("sample-maya", "Maya Patel", true),
                ),
            maxMembers = 100,
        )
    val loading = populated.copy(busy = true)
    val review = populated.copy(step = CreateStep.REVIEW)
    val failure =
        populated.copy(
            failure =
                AppFailure(
                    "Cannot create this conversation. Try again.",
                    "create-unavailable",
                    FailureRecovery.RETRY,
                ),
        )
}

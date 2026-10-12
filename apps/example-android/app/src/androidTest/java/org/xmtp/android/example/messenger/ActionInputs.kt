package org.xmtp.android.example.messenger

import org.xmtp.android.example.shared.MessengerAction
import org.xmtp.android.example.shared.feature.chat.ChatAction
import org.xmtp.android.example.shared.feature.group.GroupAction

/** Match captured input. Native readbacks supply the result assertions. */
internal fun ChatAction.matchesInput(expected: MessengerAction): Boolean =
    when (expected) {
        is MessengerAction.SendText -> {
            this is ChatAction.SendText && text == expected.text &&
                requestId == expected.requestId
        }

        is MessengerAction.RetrySend -> {
            this == ChatAction.RetrySend(expected.messageId)
        }

        is MessengerAction.React -> {
            this == ChatAction.React(expected.messageId, expected.emoji, expected.remove)
        }

        is MessengerAction.DeleteMessage -> {
            this == ChatAction.DeleteMessage(expected.messageId)
        }

        is MessengerAction.Viewport -> {
            this == ChatAction.Viewport(expected.anchor, expected.atNewest)
        }

        MessengerAction.Refresh -> {
            this == ChatAction.Refresh
        }

        MessengerAction.LoadOlder -> {
            this == ChatAction.LoadOlder
        }

        MessengerAction.JumpToLatest -> {
            this == ChatAction.JumpToLatest
        }

        MessengerAction.LoadOlderRecovery -> {
            this == ChatAction.LoadOlderRecovery
        }

        MessengerAction.LatestRecovery -> {
            this == ChatAction.LatestRecovery
        }

        else -> {
            false
        }
    }

internal fun GroupAction.matchesInput(expected: MessengerAction): Boolean =
    when (expected) {
        is MessengerAction.UpdateGroup -> this == GroupAction.UpdateGroup(expected.name, expected.description)
        is MessengerAction.SetPreset -> this == GroupAction.SetPreset(expected.adminOnly)
        is MessengerAction.AddMember -> this == GroupAction.AddMember(expected.inboxId)
        is MessengerAction.RemoveMember -> this == GroupAction.RemoveMember(expected.inboxId)
        is MessengerAction.SetAdmin -> this == GroupAction.SetAdmin(expected.inboxId, expected.admin)
        is MessengerAction.SetDisappearing -> this == GroupAction.SetDisappearing(expected.seconds)
        MessengerAction.RequestRemoval -> this == GroupAction.RequestRemoval
        is MessengerAction.SetConversationNotifications -> this == GroupAction.SetNotifications(expected.enabled)
        else -> false
    }

internal fun MessengerAction.isGroupInput(): Boolean =
    this is MessengerAction.UpdateGroup || this is MessengerAction.SetPreset ||
        this is MessengerAction.AddMember || this is MessengerAction.RemoveMember ||
        this is MessengerAction.SetAdmin || this is MessengerAction.SetDisappearing ||
        this == MessengerAction.RequestRemoval || this is MessengerAction.SetConversationNotifications

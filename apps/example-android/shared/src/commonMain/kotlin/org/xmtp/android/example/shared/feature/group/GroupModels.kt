package org.xmtp.android.example.shared.feature.group

import org.xmtp.android.example.shared.*

enum class GroupEdit { NONE, DETAILS, MEMBERS, PERMISSIONS, DISAPPEARING, LEAVE }

data class GroupState(
    val conversationId: String? = null,
    val settings: ConversationSettings = ConversationSettings(),
    val name: String = "",
    val description: String = "",
    val memberInput: String = "",
    val disappearingInput: String = "0",
    val editing: GroupEdit = GroupEdit.NONE,
    val pendingMemberIds: List<String> = emptyList(),
    val pendingLeave: Boolean = false,
    val sharedMedia: List<MessageRow> = emptyList(),
    val metadata: CapabilityUi = CapabilityUi(),
    val notifications: CapabilityUi = CapabilityUi(),
    val leave: CapabilityUi = CapabilityUi(),
    val busy: Boolean = false,
    val failure: AppFailure? = null,
    val alias: String = "",
    val aliasInput: String = "",
)

sealed interface GroupAction {
    data class ChangeAlias(
        val value: String,
    ) : GroupAction

    data class SetAlias(
        val value: String,
    ) : GroupAction

    data class ChangeName(
        val value: String,
    ) : GroupAction

    data class ChangeDescription(
        val value: String,
    ) : GroupAction

    data class ChangeMemberInput(
        val value: String,
    ) : GroupAction

    data class ChangeDisappearingInput(
        val value: String,
    ) : GroupAction

    data class Edit(
        val value: GroupEdit,
    ) : GroupAction

    data object CancelEdit : GroupAction

    data class UpdateGroup(
        val name: String,
        val description: String,
    ) : GroupAction

    data class SetPreset(
        val adminOnly: Boolean,
    ) : GroupAction

    data class AddMember(
        val inboxId: String,
    ) : GroupAction

    data class RemoveMember(
        val inboxId: String,
    ) : GroupAction

    data class SetAdmin(
        val inboxId: String,
        val admin: Boolean,
    ) : GroupAction

    data class SetDisappearing(
        val seconds: Long,
    ) : GroupAction

    data class SetNotifications(
        val enabled: Boolean,
    ) : GroupAction

    data object RequestRemoval : GroupAction

    data object Block : GroupAction

    data object Refresh : GroupAction
}

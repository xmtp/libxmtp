package org.xmtp.android.example.shared.feature.group

import org.xmtp.android.example.shared.*

object GroupFixtures {
    val empty = GroupState()
    val populated =
        GroupState(
            conversationId = "conversation-team",
            settings =
                ConversationSettings(
                    title = "Project team",
                    description = "Work and meeting notes",
                    group = true,
                    members =
                        listOf(
                            MemberRow("sample-alex", "Admin", true, "Alex Chen"),
                            MemberRow("sample-maya", "Member", true, "Maya Patel"),
                        ),
                    preset = "All members",
                    membership = "Allowed",
                    canRequestRemoval = true,
                    notifications = true,
                ),
            name = "Project team",
            description = "Work and meeting notes",
            metadata = CapabilityUi(true),
            notifications = CapabilityUi(true),
            leave = CapabilityUi(true),
        )
    val loading = populated.copy(busy = true)
    val failure =
        populated.copy(
            failure = AppFailure("Cannot update this group. Try again.", "group-unavailable", FailureRecovery.RETRY),
        )
    val pendingLeave = populated.copy(pendingLeave = true, editing = GroupEdit.LEAVE)
    val permissionFailure = populated.copy(metadata = CapabilityUi(false, "This server does not support group fields."))
    val pendingMember = populated.copy(pendingMemberIds = listOf("sample-maya"))
}

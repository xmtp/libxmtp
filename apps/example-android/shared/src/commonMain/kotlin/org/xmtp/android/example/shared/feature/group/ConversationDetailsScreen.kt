package org.xmtp.android.example.shared.feature.group
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.*

@Composable fun ConversationDetailsScreen(
    state: GroupState,
    action: (GroupAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    val name = state.name
    val description = state.description
    val member = state.memberInput
    val seconds = state.disappearingInput
    LazyColumn(
        Modifier.testTag("conversation-settings"),
        contentPadding =
            PaddingValues(
                20.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    12.dp,
                ),
    ) {
        if (state.settings.group) {
            item {
                Column(
                    verticalArrangement =
                        Arrangement
                            .spacedBy(
                                12.dp,
                            ),
                ) {
                    OutlinedTextField(
                        name,
                        {
                            action(GroupAction.ChangeName(it))
                        },
                        label = {
                            Text("Name")
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth(),
                    )
                    OutlinedTextField(
                        description,
                        {
                            action(GroupAction.ChangeDescription(it))
                        },
                        label = {
                            Text("Description")
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth(),
                    )
                    Action("Save") {
                        action(
                            GroupAction
                                .UpdateGroup(
                                    name,
                                    description,
                                ),
                        )
                    }
                    Text("Permissions: ${state.settings.preset}")
                    Action("All members") {
                        action(
                            GroupAction
                                .SetPreset(false),
                        )
                    }
                    Action("Admins only") {
                        action(
                            GroupAction
                                .SetPreset(true),
                        )
                    }
                    Text("Membership: ${state.settings.membership}")
                }
            }
        }
        item {
            Column {
                OutlinedTextField(
                    seconds,
                    {
                        action(GroupAction.ChangeDisappearingInput(it))
                    },
                    label = {
                        Text("Disappearing messages: seconds (0 is Off)")
                    },
                    modifier =
                        Modifier
                            .fillMaxWidth(),
                )
                TextButton(
                    {
                        seconds
                            .toLongOrNull()
                            ?.let {
                                action(
                                    GroupAction
                                        .SetDisappearing(it),
                                )
                            }
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                    enabled =
                        seconds
                            .toLongOrNull()
                            ?.let {
                                it >= 0
                            } == true,
                ) {
                    Text("Save duration")
                }
            }
        }
        item {
            Text(
                "Members",
                fontWeight =
                    FontWeight.Bold,
            )
        }
        items(
            state.settings.members,
            key = {
                it.inboxId
            },
        ) { row ->
            Column(Modifier.testTag("settings-member-${row.inboxId}")) {
                Text(
                    row.inboxId,
                )
                Text(
                    row.role,
                )
                if (row.canManage) {
                    Row {
                        TextButton(
                            {
                                action(
                                    GroupAction
                                        .SetAdmin(
                                            row.inboxId,
                                            row.role != "Admin",
                                        ),
                                )
                            },
                            Modifier
                                .heightIn(
                                    min =
                                        48.dp,
                                ),
                        ) {
                            Text(
                                if (row.role == "Admin") {
                                    "Remove admin"
                                } else {
                                    "Make admin"
                                },
                            )
                        }
                        TextButton(
                            {
                                action(
                                    GroupAction
                                        .RemoveMember(
                                            row.inboxId,
                                        ),
                                )
                            },
                            Modifier
                                .heightIn(
                                    min =
                                        48.dp,
                                ),
                        ) {
                            Text("Remove")
                        }
                    }
                }
            }
        }
        if (state.settings.group) {
            item {
                Column {
                    OutlinedTextField(
                        member,
                        {
                            action(GroupAction.ChangeMemberInput(it))
                        },
                        label = {
                            Text("Inbox ID")
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth(),
                    )
                    Action(
                        "Add member",
                        member
                            .isNotBlank(),
                    ) {
                        action(
                            GroupAction
                                .AddMember(member),
                        )
                    }
                }
            }
        }
        if (state.metadata.available) {
            item {
                Column {
                    if (state.settings.group) {
                        Action("Group fields") {
                            state.conversationId?.let { shellAction(ShellAction.Navigate(AppRoute.GroupFields(it))) }
                        }
                    }
                    Action("My fields") {
                        state.conversationId?.let { shellAction(ShellAction.Navigate(AppRoute.OwnFields(it))) }
                    }
                }
            }
        }
        if (state.notifications.available) {
            item {
                val label = if (state.settings.notifications) "Turn notifications off" else "Turn notifications on"
                Action(label) {
                    action(
                        GroupAction.SetNotifications(!state.settings.notifications),
                    )
                }
            }
        }
        item {
            Action("Block") {
                action(
                    GroupAction.Block,
                )
            }
        }
        if (state.settings.canRequestRemoval) {
            item {
                Action("Request removal") {
                    action(
                        GroupAction.RequestRemoval,
                    )
                }
            }
        }
    }
}

package org.xmtp.android.example.shared.feature.conversations
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.distinctUntilChanged
import org.xmtp.android.example.shared.*

@Composable fun ConversationsScreen(
    state: ConversationsState,
    action: (ConversationsAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    val list = rememberLazyListState(state.anchor.firstIndex, state.anchor.offsetPx)
    LaunchedEffect(
        state.unknownTab,
    ) {
        snapshotFlow {
            Triple(list.firstVisibleItemIndex, list.layoutInfo.visibleItemsInfo.size, list.firstVisibleItemScrollOffset)
        }.distinctUntilChanged().collect {
            (
                index,
                count,
                offset,
            ),
            ->
            if (count > 0) {
                action(
                    ConversationsAction
                        .Viewport(
                            index,
                            count,
                            offset,
                        ),
                )
            }
        }
    }
    Column {
        if (state.connection
                .isNotBlank()
        ) {
            Notice(
                state.connection,
            )
        }
        Row(
            Modifier
                .fillMaxWidth(),
        ) {
            listOf(
                false to "Allowed",
                true to "Unknown",
            ).forEach {
                (
                    unknown,
                    title,
                ),
                ->
                TextButton(
                    {
                        action(
                            ConversationsAction
                                .SelectTab(unknown),
                        )
                    },
                    Modifier
                        .weight(1f)
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text(
                        title,
                        color =
                            if (state.unknownTab == unknown) {
                                blue
                            } else {
                                ink
                            },
                    )
                }
            }
        }
        LazyColumn(state = list) {
            if (state.rows.none {
                    it.unknown ==
                        state.unknownTab
                }
            ) {
                item {
                    Text(
                        "No conversations",
                        Modifier
                            .padding(
                                24.dp,
                            ),
                    )
                }
            }
            items(
                state.rows.filter {
                    it.unknown ==
                        state.unknownTab
                },
                key = {
                    it.id
                },
            ) { row ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clickable {
                            action(
                                ConversationsAction
                                    .OpenConversation(
                                        row.id,
                                    ),
                            )
                        }.padding(
                            16.dp,
                        ),
                    verticalAlignment =
                        Alignment.CenterVertically,
                ) {
                    Box(
                        Modifier
                            .size(
                                48.dp,
                            ).background(
                                if (row
                                        .pattern % 2 == 0
                                ) {
                                    outgoing
                                } else {
                                    ink
                                },
                                CircleShape,
                            ),
                        contentAlignment =
                            Alignment.Center,
                    ) {
                        Text(
                            if (row
                                    .pattern % 2 == 0
                            ) {
                                "╳"
                            } else {
                                "≋"
                            },
                            color = blue,
                        )
                    }
                    Column(
                        Modifier
                            .weight(1f)
                            .padding(
                                horizontal =
                                    12.dp,
                            ),
                    ) {
                        Text(
                            row.title,
                            fontWeight =
                                FontWeight.Bold,
                        )
                        Text(
                            row.preview,
                            maxLines = 1,
                            style =
                                MaterialTheme.typography.bodyMedium,
                        )
                    }
                    Column(
                        horizontalAlignment =
                            Alignment.End,
                    ) {
                        Text(
                            row.time,
                            style =
                                MaterialTheme.typography.labelSmall,
                        )
                        if (row.unread != "0") {
                            Text(
                                row.unread,
                                color = blue,
                            )
                        }
                    }
                }
                HorizontalDivider()
            }
            item {
                TextButton(
                    {
                        action(
                            ConversationsAction.LoadMore,
                        )
                    },
                    Modifier
                        .fillMaxWidth()
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Load more")
                }
            }
        }
    }
}

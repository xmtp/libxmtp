package org.xmtp.android.example.shared.feature.conversations

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.distinctUntilChanged
import org.xmtp.android.example.shared.*

@Composable
fun ConversationsScreen(
    state: ConversationsState,
    action: (ConversationsAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
) {
    val query = state.query.trim()
    val source =
        if (state.unknownTab) {
            state.requests.ifEmpty {
                state.rows.filter { it.unknown }
            }
        } else {
            state.rows.filter { !it.unknown }
        }
    val rows = source
    val list = rememberLazyListState(state.anchor.firstIndex, state.anchor.offsetPx)
    LaunchedEffect(list, state.unknownTab) {
        snapshotFlow {
            Triple(list.firstVisibleItemIndex, list.layoutInfo.visibleItemsInfo.size, list.firstVisibleItemScrollOffset)
        }.distinctUntilChanged()
            .collect { (index, count, offset) ->
                if (count >
                    0
                ) {
                    action(
                        ConversationsAction.Viewport(
                            index,
                            count,
                            offset,
                            firstConversationIndex = (index - 1).coerceAtLeast(0),
                        ),
                    )
                }
            }
    }
    LaunchedEffect(list, rows.size, state.hasMore, state.pageLoading, state.busy, state.failure, query) {
        snapshotFlow {
            list.layoutInfo.visibleItemsInfo
                .lastOrNull()
                ?.index ?: -1
        }.distinctUntilChanged()
            .collect { last ->
                if (rows.isNotEmpty() && last >= rows.lastIndex - 3 && query.isEmpty() && state.hasMore &&
                    !state.pageLoading &&
                    !state.busy &&
                    state.failure == null
                ) {
                    action(ConversationsAction.LoadMore)
                }
            }
    }
    LazyColumn(state = list, modifier = Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 24.dp)) {
        item(key = "search") {
            Column(
                Modifier.padding(horizontal = 20.dp, vertical = 12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                val connectionNotice =
                    when (state.connection.uppercase()) {
                        "CONNECTING" -> "Connecting to your server…"
                        "RECONNECTING" -> "Reconnecting to your server…"
                        "FAILED" -> "Cannot connect to your server."
                        "CLOSED" -> "Not connected to your server."
                        else -> null
                    }
                connectionNotice?.let { Notice(it) }
                state.failure?.let { ConversationFailure(it, state.busy) { action(ConversationsAction.Refresh) } }
                OutlinedTextField(
                    value = state.query,
                    onValueChange = { action(ConversationsAction.ChangeQuery(it)) },
                    label = { Text(if (state.unknownTab) "Search requests" else "Search chats") },
                    supportingText = { Text("Search conversations loaded on this device.") },
                    trailingIcon =
                        if (state.query.isNotEmpty()) {
                            {
                                TextButton(onClick = {
                                    action(ConversationsAction.ChangeQuery(""))
                                }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Clear") }
                            }
                        } else {
                            null
                        },
                    singleLine = true,
                    shape = RoundedCornerShape(18.dp),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (state.unknownTab) {
                    Text(
                        "Messages from people you have not accepted yet.",
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    TextButton(onClick = {
                        action(ConversationsAction.SelectTab(false))
                    }, modifier = Modifier.heightIn(min = 48.dp)) { Text("Back to chats") }
                } else {
                    Surface(onClick = {
                        action(ConversationsAction.OpenRequests)
                    }, color = MaterialTheme.colorScheme.primaryContainer, shape = RoundedCornerShape(18.dp)) {
                        Column(
                            Modifier.fillMaxWidth().padding(16.dp),
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            val requestLabel =
                                if (state.requestCountValid && state.requestCount != null) {
                                    "Message requests · ${state.requestCount}"
                                } else {
                                    "Message requests"
                                }
                            Text(
                                requestLabel,
                                style = MaterialTheme.typography.titleMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer,
                            )
                            Text(
                                "Choose who can contact you",
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer,
                            )
                        }
                    }
                }
            }
        }
        if (rows.isEmpty()) {
            item(key = "empty") {
                if (state.busy) {
                    Column(
                        Modifier.fillMaxWidth().padding(40.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(16.dp),
                    ) {
                        CircularProgressIndicator(Modifier.size(28.dp))
                        Text("Loading conversations…", style = MaterialTheme.typography.bodyLarge)
                    }
                } else if (query.isNotEmpty()) {
                    ConversationEmpty("No matches", "Try another name or message. Search covers loaded conversations.")
                } else if (state.unknownTab) {
                    ConversationEmpty(
                        "No message requests",
                        "New requests will appear here. You can review them before you reply.",
                    )
                } else {
                    ConversationEmpty(
                        "Your chats start here",
                        "Send a message to someone you know. Messages from new people appear in requests.",
                        "Start a conversation",
                    ) {
                        action(ConversationsAction.Create(false))
                    }
                }
            }
        }
        items(rows, key = { it.id }) { row ->
            ConversationItem(
                row,
                state.selectedConversationId == row.id,
            ) { action(ConversationsAction.OpenConversation(row.id)) }
            HorizontalDivider(
                Modifier.padding(start = 82.dp, end = 20.dp),
                color = MaterialTheme.colorScheme.outlineVariant,
            )
        }
        if (state.pageLoading) {
            item(key = "page-progress") {
                Row(
                    Modifier.fillMaxWidth().padding(24.dp),
                    horizontalArrangement = Arrangement.Center,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(12.dp))
                    Text("Loading more…", style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}

package org.xmtp.android.example.shared.feature.chat

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import kotlinx.coroutines.flow.distinctUntilChanged
import org.xmtp.android.example.shared.*
import org.xmtp.android.example.shared.designsystem.*
import kotlin.math.roundToInt

@Composable
fun ChatScreen(
    state: ChatState,
    action: (ChatAction) -> Unit,
    shellAction: (ShellAction) -> Unit = {},
    composerExtra: @Composable () -> Unit = {},
    messageExtra: @Composable (MessageRow) -> Unit = {},
) {
    val list = key(state.conversationId) { rememberLazyListState() }
    val latestState by rememberUpdatedState(state)
    val latestAction by rememberUpdatedState(action)
    val bubbleBounds = remember(state.conversationId) { mutableStateMapOf<String, Rect>() }
    var composerTop by remember(state.conversationId) { mutableStateOf(Float.POSITIVE_INFINITY) }
    val selected = state.messages.firstOrNull { it.id == state.overlay?.messageId && !it.deleted }
    val acceptedMessageId =
        when (
            val outcome =
                state.composer.result
                    ?.takeIf { it.conversationId == state.conversationId }
                    ?.outcome
        ) {
            is TextSendOutcome.Accepted -> outcome.messageId
            is TextSendOutcome.AcceptedPendingCommit -> outcome.messageId
            else -> null
        }
    val reducedMotion = LocalReducedMotion.current
    val duration = if (reducedMotion) 0 else MessengerMotion.ROW
    val popupTransition = remember(state.conversationId) { MutableTransitionState(false) }
    var popupMessageId by remember(state.conversationId) { mutableStateOf<String?>(null) }
    var popupInstanceId by remember(state.conversationId) { mutableStateOf(0L) }
    val requestedPopover = state.overlay?.takeIf { it.surface == MessageSurface.POPOVER }
    val requestedPopup = selected?.takeIf { state.overlay?.surface == MessageSurface.POPOVER }
    LaunchedEffect(requestedPopup?.id, requestedPopover?.instanceId) {
        if (requestedPopup != null) {
            popupMessageId = requestedPopup.id
            popupInstanceId = requestedPopover?.instanceId ?: 0L
            popupTransition.targetState = true
        } else {
            popupTransition.targetState = false
        }
    }
    LaunchedEffect(popupTransition.currentState, popupTransition.isIdle) {
        if (popupTransition.isIdle && !popupTransition.currentState &&
            !popupTransition.targetState
        ) {
            popupMessageId = null
        }
    }
    var reactorOverlay by remember(state.conversationId) { mutableStateOf<MessageOverlayState?>(null) }
    val requestedReactors = state.overlay?.takeIf { it.surface == MessageSurface.REACTORS }
    LaunchedEffect(requestedReactors) { if (requestedReactors != null) reactorOverlay = requestedReactors }
    val reactorRow = state.messages.firstOrNull { it.id == reactorOverlay?.messageId && !it.deleted }
    val popupRow = state.messages.firstOrNull { it.id == popupMessageId && !it.deleted }
    var initialRestoreComplete by remember(state.conversationId) { mutableStateOf(false) }
    LaunchedEffect(
        state.conversationId,
        state.anchor?.messageId,
        state.messages.any { it.id == state.anchor?.messageId },
    ) {
        state.anchor?.takeIf { !initialRestoreComplete }?.let { anchor ->
            val index = state.messages.indexOfFirst { it.id == anchor.messageId }
            if (index >= 0) {
                list.scrollToItem(index, anchor.offsetPx)
                initialRestoreComplete = true
            }
        }
    }
    LaunchedEffect(state.conversationId) {
        snapshotFlow {
            val index = list.firstVisibleItemIndex
            val offset = list.firstVisibleItemScrollOffset
            val nativeKey =
                list.layoutInfo.visibleItemsInfo
                    .firstOrNull { it.index == index }
                    ?.key
            val pendingAnchor = !initialRestoreComplete && latestState.anchor != null
            latestState.messages.getOrNull(index)?.takeIf { !pendingAnchor && it.id == nativeKey }?.let { row ->
                ScrollAnchor(row.id, row.sentAtNs, offset, index == 0 && offset == 0)
            }
        }.distinctUntilChanged().collect { anchor ->
            anchor?.let {
                initialRestoreComplete = true
                latestAction(ChatAction.Viewport(it, it.wasAtNewest))
            }
        }
    }
    Column(Modifier.fillMaxSize()) {
        when (val outcome = state.composer.result?.outcome) {
            is TextSendOutcome.AcceptedPendingCommit -> {
                Notice("Message saved. Finishing the send…", "Retry") {
                    action(ChatAction.RetrySend(outcome.messageId))
                }
            }

            is TextSendOutcome.Rejected -> {
                Notice(outcome.failure.message)
            }

            is TextSendOutcome.Unknown -> {
                Notice(
                    "We could not confirm whether this message was sent. Your draft is kept. " +
                        "Check the conversation before you try again.",
                    "Review send",
                ) {
                    shellAction(ShellAction.Navigate(AppRoute.Settings))
                }
            }

            else -> {
                Unit
            }
        }
        state.readerFailure?.let { Notice(it.message, "Retry reader") { shellAction(ShellAction.RetryReader) } }
        state.failure?.let { failure ->
            if (failure.code == "send-publication") {
                Box(Modifier.fillMaxWidth().testTag("publication-recovery")) {
                    Notice(failure.message, if (acceptedMessageId != null) "Retry" else null) {
                        acceptedMessageId?.let { action(ChatAction.RetrySend(it)) }
                    }
                }
            } else {
                Notice(failure.message, "Retry") { shellAction(ShellAction.Refresh) }
            }
        }
        state.historyNotice?.let { Notice(it) }
        state.recoveryNotice?.let { Notice(it) }
        if (state.hasOlderRecovery || !state.recoveryAtNewest) {
            Row(Modifier.fillMaxWidth()) {
                if (state.hasOlderRecovery) {
                    TextButton({
                        action(ChatAction.LoadOlderRecovery)
                    }, Modifier.weight(1f).heightIn(min = 48.dp)) { Text("Older pending messages") }
                }
                if (!state.recoveryAtNewest) {
                    TextButton({
                        action(ChatAction.LatestRecovery)
                    }, Modifier.weight(1f).heightIn(min = 48.dp)) { Text("Newest pending messages") }
                }
            }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            LazyColumn(
                state = list,
                reverseLayout = true,
                modifier = Modifier.fillMaxSize().testTag("message-timeline"),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 16.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                itemsIndexed(state.messages, key = { _, row -> row.id }) { index, row ->
                    val older = state.messages.getOrNull(index + 1)
                    val senderStart =
                        older == null || older.sender != row.sender || older.day != row.day || older.mine != row.mine
                    Column(
                        Modifier.animateItem(
                            fadeInSpec = tween(duration),
                            placementSpec = tween(duration),
                            fadeOutSpec = tween(duration),
                        ),
                    ) {
                        if (older == null || older.day != row.day) {
                            Box(
                                Modifier.fillMaxWidth().padding(vertical = 18.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Surface(
                                    shape = RoundedCornerShape(16.dp),
                                    color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = .6f),
                                ) {
                                    Text(
                                        row.day,
                                        Modifier.padding(horizontal = 14.dp, vertical = 6.dp),
                                        style = MaterialTheme.typography.labelMedium,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                        }
                        if (senderStart) Spacer(Modifier.height(10.dp))
                        MessageBubble(
                            row,
                            state.group && senderStart,
                            selected?.id == row.id,
                            state.composer.reply?.messageId != row.id,
                            !state.unknown,
                            action,
                            { bubbleBounds[row.id] = it },
                            messageExtra,
                        )
                    }
                }
                if (state.hasOlder) {
                    item {
                        TextButton({
                            action(ChatAction.LoadOlder)
                        }, Modifier.fillMaxWidth().heightIn(min = 48.dp), enabled = !state.busy) {
                            Text(if (state.busy) "Loading history…" else "Older messages")
                        }
                    }
                }
            }
            if (state.messages.isEmpty() &&
                !state.busy
            ) {
                Column(
                    Modifier.align(Alignment.Center).padding(32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    MessengerAvatar(state.title, size = 64.dp, group = state.group)
                    Spacer(Modifier.height(16.dp))
                    Text("Start the conversation", style = MaterialTheme.typography.titleLarge)
                    Text(
                        "Send your first message below.",
                        Modifier.padding(top = 8.dp),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            androidx.compose.animation.AnimatedVisibility(
                !state.atNewest,
                Modifier.align(Alignment.BottomEnd).padding(16.dp),
                enter = fadeIn(tween(duration)),
                exit = fadeOut(tween(duration)),
            ) {
                FilledTonalButton({
                    action(ChatAction.JumpToLatest)
                }, Modifier.heightIn(min = 48.dp), shape = RoundedCornerShape(24.dp)) {
                    Icon(AppIcons.Down, "Jump to latest")
                    state.newMessageCount?.takeIf { it > 0 }?.let { Text("$it new", Modifier.padding(start = 8.dp)) }
                }
            }
        }
        Box(
            Modifier.onGloballyPositioned {
                composerTop = it.boundsInWindow().top
            },
        ) { MessageComposer(state, action, composerExtra) }
    }
    if (popupRow != null && (popupTransition.currentState || popupTransition.targetState)) {
        val gap = with(LocalDensity.current) { 8.dp.toPx() }
        val retainedInstance = popupInstanceId
        val popupAction: (ChatAction) -> Unit = { value ->
            val current = latestState.overlay
            if (current != null && current.messageId == popupRow.id && current.surface == MessageSurface.POPOVER &&
                current.instanceId == retainedInstance
            ) {
                action(value)
            }
        }
        Popup(MessagePopupPosition(bubbleBounds[popupRow.id] ?: Rect.Zero, composerTop, gap), onDismissRequest = {
            popupAction(ChatAction.DismissOverlay)
        }, properties = PopupProperties(focusable = popupTransition.targetState)) {
            val pickerDuration = if (reducedMotion) 0 else MessengerMotion.PICKER
            AnimatedVisibility(
                visibleState = popupTransition,
                enter =
                    fadeIn(tween(pickerDuration)) +
                        scaleIn(tween(pickerDuration), initialScale = if (reducedMotion) 1f else .98f),
                exit = fadeOut(tween(pickerDuration)),
            ) {
                MessageActions(
                    popupRow,
                    with(LocalDensity.current) {
                        (composerTop - gap).toDp().coerceAtLeast(48.dp)
                    },
                    popupTransition.targetState,
                    !state.unknown,
                    popupAction,
                )
            }
        }
    }
    val retainedReactors = reactorOverlay
    if (reactorRow != null && retainedReactors != null) {
        ReactionDetails(
            reactorRow,
            retainedReactors.emoji,
            retainedReactors.instanceId,
            requestedReactors != null,
            !state.unknown,
            {
                if (reactorOverlay?.instanceId == retainedReactors.instanceId &&
                    reactorOverlay?.messageId == retainedReactors.messageId
                ) {
                    reactorOverlay = null
                }
            },
        ) { value ->
            val current = latestState.overlay
            if (value is ChatAction.CloseSheet ||
                (
                    current != null && current.messageId == retainedReactors.messageId &&
                        current.surface == MessageSurface.REACTORS &&
                        current.instanceId == retainedReactors.instanceId
                )
            ) {
                action(value)
            }
        }
    }
}

private class MessagePopupPosition(
    private val bubble: Rect,
    private val composerTop: Float,
    private val gap: Float,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val x =
            (bubble.center.x - popupContentSize.width / 2).roundToInt().coerceIn(
                0,
                (
                    windowSize.width -
                        popupContentSize.width
                ).coerceAtLeast(0),
            )
        val bottom = minOf(windowSize.height.toFloat(), composerTop) - gap
        val above = bubble.top - popupContentSize.height - gap
        val below = bubble.bottom + gap
        val y =
            (
                if (above >=
                    0
                ) {
                    above
                } else if (below + popupContentSize.height <=
                    bottom
                ) {
                    below
                } else {
                    bottom - popupContentSize.height
                }
            ).roundToInt().coerceIn(
                0,
                (
                    bottom -
                        popupContentSize.height
                ).roundToInt().coerceAtLeast(0),
            )
        return IntOffset(x, y)
    }
}

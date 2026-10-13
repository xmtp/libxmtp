package org.xmtp.android.example.shared.feature.handles

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import org.xmtp.android.example.shared.AppIcons
import org.xmtp.android.example.shared.MessengerAvatar
import org.xmtp.android.example.shared.Notice
import org.xmtp.android.example.shared.XmtpBrand

/** The onboarding step after Join a server. A claim or Skip for now opens Chats. */
@Composable
fun ChooseHandleScreen(
    state: HandleState,
    action: (HandleAction) -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 32.dp)
            .testTag("choose-handle"),
        verticalArrangement = Arrangement.spacedBy(20.dp),
    ) {
        XmtpBrand()
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(HandleCopy.TITLE, style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
            Text(
                HandleCopy.INTRO,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        // An inbox whose handles are unknown can already have one: offer no claim until a read works.
        if (!state.ownUnknown) {
            Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surface) {
                Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(
                        HandleCopy.CARD_TITLE,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    HandleEditor(state, action)
                }
            }
        }
        state.notice?.let { NoticeBox(it, state, action) }
        if (!state.ownUnknown) {
            Button(
                onClick = { primary(state, action) },
                enabled = state.canClaim || state.canCheck,
                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp),
                shape = RoundedCornerShape(18.dp),
            ) {
                if (state.claiming != null) {
                    CircularProgressIndicator(
                        Modifier.size(20.dp),
                        color = LocalContentColor.current,
                        strokeWidth = 2.dp,
                    )
                    Spacer(Modifier.width(12.dp))
                }
                Text(HandleCopy.primary(state), style = MaterialTheme.typography.titleMedium)
            }
        }
        TextButton(
            onClick = { action(HandleAction.Skip) },
            enabled = state.claiming == null,
            modifier = Modifier.align(Alignment.CenterHorizontally).heightIn(min = 48.dp),
        ) { Text(HandleCopy.SKIP) }
        Text(
            HandleCopy.CHOOSE_FOOTER,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** Settings > Change handle. It reuses the onboarding editor and confirms before each claim. */
@Composable
fun ChangeHandleScreen(
    state: HandleState,
    action: (HandleAction) -> Unit,
) {
    val limit = state.notice == HandleNotice.LimitReached
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 32.dp)
            .testTag("change-handle"),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        if (state.loading) LinearProgressIndicator(Modifier.fillMaxWidth())
        if (state.ownUnknown) {
            // The current and other handles are unknown: show the error in their place, and no editor.
            state.notice?.let { NoticeBox(it, state, action) }
            return@Column
        }
        state.current?.let { current ->
            HandleSection(HandleCopy.CURRENT_TITLE) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    MessengerAvatar(current, size = 48.dp)
                    Column(Modifier.weight(1f)) {
                        HandleText(current, MaterialTheme.typography.titleMedium)
                        state.currentSince?.let {
                            Text(
                                HandleCopy.since(it),
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
                if (limit) Notice(HandleCopy.LIMIT_REACHED)
            }
        }
        if (!limit) {
            HandleSection(if (state.current == null) HandleCopy.CARD_TITLE else HandleCopy.NEW_TITLE) {
                state.notice?.takeIf { it is HandleNotice.TooSoon }?.let { NoticeBox(it, state, action) }
                HandleEditor(state, action)
                state.notice
                    ?.takeIf { it !is HandleNotice.TooSoon && it !is HandleNotice.SwitchBackBlocked }
                    ?.let { NoticeBox(it, state, action) }
                Button(
                    onClick = { primary(state, action) },
                    enabled = state.canClaim || state.canCheck,
                    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp),
                ) {
                    if (state.claiming != null) {
                        CircularProgressIndicator(
                            Modifier.size(20.dp),
                            color = LocalContentColor.current,
                            strokeWidth = 2.dp,
                        )
                        Spacer(Modifier.width(12.dp))
                    }
                    Text(HandleCopy.primary(state))
                }
            }
        }
        if (state.others.isNotEmpty()) {
            HandleSection(HandleCopy.OTHERS_TITLE) {
                Text(
                    if (limit) HandleCopy.OTHERS_LIMIT_TEXT else HandleCopy.OTHERS_TEXT,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                val rows = if (limit) state.others.take(HANDLE_SUGGESTION_PAGE) else state.others
                rows.forEachIndexed { index, handle ->
                    if (index > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 48.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        HandleText(handle, MaterialTheme.typography.bodyLarge, Modifier.weight(1f))
                        if (!limit) {
                            OutlinedButton(
                                onClick = { action(HandleAction.SwitchBack(handle)) },
                                enabled = state.editable,
                                modifier =
                                    Modifier.heightIn(min = 48.dp).semantics {
                                        contentDescription = "${HandleCopy.USE} $handle"
                                    },
                            ) { Text(HandleCopy.USE) }
                        }
                    }
                }
                if (limit && state.others.size > rows.size) {
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    Text(
                        HandleCopy.more(state.others.size - rows.size),
                        Modifier.heightIn(min = 48.dp).wrapContentHeight(Alignment.CenterVertically),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                state.notice?.takeIf { it is HandleNotice.SwitchBackBlocked }?.let { NoticeBox(it, state, action) }
            }
        }
        if (!limit) {
            Text(
                HandleCopy.CHANGE_FOOTER,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    state.confirm?.let { handle ->
        AlertDialog(
            onDismissRequest = { action(HandleAction.CancelClaim) },
            title = { Text(HandleCopy.confirmTitle(state)) },
            text = { Text(HandleCopy.confirmText(state, handle)) },
            confirmButton = {
                TextButton({ action(HandleAction.ConfirmClaim) }) { Text(HandleCopy.confirmButton(state)) }
            },
            dismissButton = { TextButton({ action(HandleAction.CancelClaim) }) { Text("Cancel") } },
        )
    }
}

/** A handle with its code in the secondary color, as the designs show it. */
@Composable
fun HandleText(
    handle: String,
    style: TextStyle,
    modifier: Modifier = Modifier,
    fontWeight: FontWeight? = null,
) {
    val code = handleCode(handle)
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Text(
        buildAnnotatedString {
            append(handleName(handle))
            if (code.isNotEmpty()) withStyle(SpanStyle(color = muted)) { append(code) }
        },
        modifier,
        style = style,
        fontWeight = fontWeight,
    )
}

private fun primary(
    state: HandleState,
    action: (HandleAction) -> Unit,
) {
    if (state.pick != null) action(HandleAction.Claim) else action(HandleAction.Check)
}

@Composable
private fun HandleEditor(
    state: HandleState,
    action: (HandleAction) -> Unit,
) {
    val problem = state.problem
    val notAllowed = state.notice is HandleNotice.NotAllowed
    OutlinedTextField(
        value = state.input,
        onValueChange = { action(HandleAction.ChangeInput(it)) },
        label = { Text(HandleCopy.FIELD_LABEL) },
        placeholder = { Text(HandleCopy.PLACEHOLDER) },
        isError = problem != null || notAllowed,
        supportingText =
            if (state.blocked) {
                null
            } else {
                {
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(
                            when {
                                problem != null -> HandleCopy.problem(problem, state.input, state.minNameLength)
                                notAllowed -> HandleCopy.NOT_ALLOWED_FIELD
                                else -> HandleCopy.supporting(state.minNameLength)
                            },
                            Modifier.weight(1f),
                        )
                        if (problem == HandleInputProblem.TOO_LONG) Text(HandleCopy.counter(state.input))
                    }
                }
            },
        keyboardOptions =
            KeyboardOptions(
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
                keyboardType = KeyboardType.Ascii,
                imeAction = ImeAction.Done,
            ),
        keyboardActions = KeyboardActions(onDone = { if (state.canCheck) action(HandleAction.Check) }),
        enabled = state.editable,
        singleLine = true,
        shape = RoundedCornerShape(16.dp),
        modifier = Modifier.fillMaxWidth().testTag("handle-input"),
    )
    when (val status = state.status) {
        HandleStatus.Editing -> {
            Unit
        }

        is HandleStatus.Checking -> {
            Row(
                Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                Text(HandleCopy.checking(status.input), style = MaterialTheme.typography.bodyMedium)
            }
        }

        is HandleStatus.Available -> {
            Row(
                Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Icon(
                    AppIcons.Check,
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                    tint = MaterialTheme.colorScheme.primary,
                )
                Text(
                    HandleCopy.available(status.handle),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.primary,
                )
            }
        }

        is HandleStatus.Suggestions -> {
            Text(
                HandleCopy.suggestions(status.input),
                Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                style = MaterialTheme.typography.bodyMedium,
            )
            Column(Modifier.selectableGroup(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                status.visible.forEach { handle ->
                    val selected = handle == status.selected
                    Surface(
                        shape = MaterialTheme.shapes.medium,
                        color =
                            if (selected) {
                                MaterialTheme.colorScheme.primaryContainer
                            } else {
                                MaterialTheme.colorScheme.surface
                            },
                        modifier =
                            Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(
                                selected = selected,
                                enabled = state.claiming == null,
                                role = Role.RadioButton,
                                onClick = { action(HandleAction.Select(handle)) },
                            ),
                    ) {
                        Row(
                            Modifier.padding(horizontal = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            RadioButton(selected, onClick = null, enabled = state.claiming == null)
                            HandleText(handle, MaterialTheme.typography.bodyLarge)
                        }
                    }
                }
            }
            if (status.more) {
                TextButton(
                    onClick = { action(HandleAction.ShowMore) },
                    enabled = state.claiming == null,
                    modifier = Modifier.heightIn(min = 48.dp),
                ) { Text(HandleCopy.SHOW_MORE) }
            }
        }

        is HandleStatus.NoneFree -> {
            Text(
                HandleCopy.noneFree(status.input),
                Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                style = MaterialTheme.typography.bodyMedium,
            )
            TextButton(
                onClick = { action(HandleAction.Check) },
                enabled = state.editable,
                modifier = Modifier.heightIn(min = 48.dp),
            ) { Text(HandleCopy.LOOK_AGAIN) }
        }
    }
}

@Composable
private fun NoticeBox(
    notice: HandleNotice,
    state: HandleState,
    action: (HandleAction) -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.errorContainer, shape = RoundedCornerShape(16.dp)) {
        Column(
            Modifier.fillMaxWidth().padding(16.dp).semantics { liveRegion = LiveRegionMode.Polite },
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                HandleCopy.notice(notice),
                color = MaterialTheme.colorScheme.onErrorContainer,
                style = MaterialTheme.typography.bodyLarge,
            )
            if (notice == HandleNotice.CheckFailed || notice == HandleNotice.LoadFailed ||
                notice is HandleNotice.ClaimFailed
            ) {
                TextButton(
                    onClick = { action(HandleAction.Retry) },
                    enabled = state.claiming == null && state.status !is HandleStatus.Checking && !state.loading,
                    modifier = Modifier.heightIn(min = 48.dp),
                    colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.onErrorContainer),
                ) { Text(HandleCopy.RETRY) }
            }
        }
    }
}

@Composable
private fun HandleSection(
    title: String,
    content: @Composable ColumnScope.() -> Unit,
) {
    Surface(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
            content()
        }
    }
}

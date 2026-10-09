package org.xmtp.android.example.shared
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.flow.distinctUntilChanged

private val ink = Color(0xFF172034)
private val blue = Color(0xFF3159E8)
private val outgoing = Color(0xFFDFE8FF)
private val xmtpLogo =
    ImageVector
        .Builder(
            "XMTP",
            64.dp,
            64.dp,
            64f,
            64f,
        ).addPath(
            pathData =
                addPathNodes(
                    "M0,32C0,14.327 14.327,0 32,0C49.662,0 63.307,14.061 63.723,31.861C63.723,37.541 61.784,42.32 56.935,46.822C52.838,50.627 45.853,51.186 40.728,48.346C37.07,46.236 34.25,41.742 31.861,38.442L27.429,45.229H17.87L26.875,31.861L18.147,18.701H27.983L31.931,25.489L35.81,18.701H45.714L36.71,31.861C36.71,31.861 41.004,38.442 43.359,41.004C45.714,43.567 50.009,43.636 52.779,40.866C55.825,37.82 56.507,35.394 56.52,31.861C56.567,18.188 45.706,7.203 32,7.203C18.305,7.203 7.203,18.305 7.203,32C7.203,45.695 18.305,56.797 32,56.797C33.894,56.797 35.71,56.637 37.472,56.242L39.134,63.238C36.627,63.8 34.463,64 32,64C14.327,64 0,49.673 0,32Z",
                ),
            fill = SolidColor(ink),
        ).build()

@Composable
fun MessengerScreens(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
    extraScreen: @Composable (Screen) -> Unit = {
    },
    composerExtra: @Composable () -> Unit = {
    },
    messageExtra: @Composable (MessageRow) -> Unit = {
    },
) {
    MaterialTheme(
        colorScheme =
            lightColorScheme(
                primary = blue,
                onPrimary =
                    Color.White,
                background =
                    Color.White,
                surface =
                    Color.White,
                onSurface = ink,
                onBackground = ink,
                onSurfaceVariant = Color(0xFF657186),
            ),
    ) {
        Surface(
            Modifier
                .fillMaxSize(),
        ) {
            Column(
                Modifier
                    .fillMaxSize()
                    .safeDrawingPadding(),
            ) {
                if (state.screen !=
                    Screen.START
                ) {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .padding(
                                horizontal =
                                    12.dp,
                            ),
                        verticalAlignment =
                            Alignment.CenterVertically,
                    ) {
                        if (state.screen !=
                            Screen.CONVERSATIONS
                        ) {
                            CircleAction(
                                "Back",
                                AppIcons.Back,
                            ) {
                                action(
                                    MessengerAction
                                        .Navigate(
                                            if (state.screen ==
                                                Screen.CONVERSATION_SETTINGS
                                            ) {
                                                Screen.TIMELINE
                                            } else {
                                                Screen.CONVERSATIONS
                                            },
                                        ),
                                )
                            }
                        }
                        Text(
                            when (
                                state.screen
                            ) {
                                Screen.CONVERSATIONS,
                                -> {
                                    "Conversations"
                                }

                                Screen.TIMELINE,
                                -> {
                                    state.conversationTitle
                                }

                                Screen.CREATE,
                                -> {
                                    "New conversation"
                                }

                                Screen.CONVERSATION_SETTINGS,
                                -> {
                                    "Conversation settings"
                                }

                                Screen.APP_SETTINGS,
                                -> {
                                    "Settings"
                                }

                                Screen.GROUP_FIELDS,
                                -> {
                                    "Group fields"
                                }

                                Screen.MY_FIELDS,
                                -> {
                                    "My fields"
                                }

                                Screen.DRAFTS,
                                -> {
                                    "Draft recovery"
                                }

                                else -> {
                                    "XMTP"
                                }
                            },
                            Modifier
                                .weight(1f)
                                .padding(
                                    12.dp,
                                ),
                            style =
                                MaterialTheme.typography.titleLarge,
                            fontWeight =
                                FontWeight.Bold,
                        )
                        if (state.screen ==
                            Screen.CONVERSATIONS
                        ) {
                            CircleAction(
                                "New conversation",
                                AppIcons.Add,
                            ) {
                                action(
                                    MessengerAction
                                        .Navigate(
                                            Screen.CREATE,
                                        ),
                                )
                            }
                            CircleAction(
                                "Settings",
                                AppIcons.Settings,
                            ) {
                                action(
                                    MessengerAction
                                        .Navigate(
                                            Screen.APP_SETTINGS,
                                        ),
                                )
                            }
                        }
                        if (state.screen ==
                            Screen.TIMELINE
                        ) {
                            CircleAction(
                                "Conversation settings",
                                AppIcons.More,
                            ) {
                                action(
                                    MessengerAction
                                        .Navigate(
                                            Screen.CONVERSATION_SETTINGS,
                                        ),
                                )
                            }
                        }
                    }
                }
                state.error?.let {
                    Notice(
                        it,
                        "Retry",
                    ) {
                        action(
                            MessengerAction.Refresh,
                        )
                    }
                }
                if (state.busy) {
                    LinearProgressIndicator(
                        Modifier
                            .fillMaxWidth(),
                    )
                }
                Box(
                    Modifier
                        .weight(1f),
                ) {
                    when (
                        state.screen
                    ) {
                        Screen.START,
                        -> {
                            Start(
                                state,
                                action,
                            )
                        }

                        Screen.CONVERSATIONS,
                        -> {
                            Conversations(
                                state,
                                action,
                            )
                        }

                        Screen.CREATE,
                        -> {
                            Create(action)
                        }

                        Screen.TIMELINE,
                        -> {
                            Timeline(
                                state,
                                action,
                                composerExtra,
                                messageExtra,
                            )
                        }

                        Screen.CONVERSATION_SETTINGS,
                        -> {
                            Settings(
                                state,
                                action,
                            )
                        }

                        Screen.APP_SETTINGS,
                        -> {
                            AppSettings(
                                state,
                                action,
                            )
                        }

                        else -> {
                            extraScreen(
                                state.screen,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable private fun CircleAction(
    label: String,
    icon: ImageVector,
    click: () -> Unit,
) {
    FilledTonalButton(
        onClick = click,
        modifier = Modifier.size(48.dp).semantics { contentDescription = label },
        shape = CircleShape,
        colors =
            ButtonDefaults.filledTonalButtonColors(
                containerColor = Color(0xFFF1F3F7),
                contentColor = ink,
            ),
        contentPadding = PaddingValues(12.dp),
    ) { Icon(icon, contentDescription = null, modifier = Modifier.size(24.dp)) }
}

@Composable private fun Notice(
    text: String,
    button: String? = null,
    click: () -> Unit = {
    },
) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(outgoing)
            .padding(
                12.dp,
            ),
        verticalAlignment =
            Alignment.CenterVertically,
    ) {
        Text(
            text,
            Modifier
                .weight(1f),
        )
        if (button != null) {
            TextButton(
                click,
                Modifier
                    .heightIn(
                        min =
                            48.dp,
                    ),
            ) {
                Text(button)
            }
        }
    }
}

@Composable private fun Action(
    label: String,
    enabled: Boolean = true,
    click: () -> Unit,
) {
    Button(
        click,
        Modifier
            .fillMaxWidth()
            .heightIn(
                min =
                    48.dp,
            ),
        enabled = enabled,
    ) {
        Text(label)
    }
}

@Composable private fun Start(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
) {
    var backend by remember(
        state.backend,
    ) {
        mutableStateOf(
            state.backend,
        )
    }
    var credential by remember {
        mutableStateOf("")
    }
    var privateNetwork by remember {
        mutableStateOf(false)
    }
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(
                24.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    16.dp,
                ),
    ) {
        Spacer(
            Modifier
                .height(
                    24.dp,
                ),
        )
        Row(
            verticalAlignment =
                Alignment.CenterVertically,
            horizontalArrangement =
                Arrangement
                    .spacedBy(
                        12.dp,
                    ),
        ) {
            Image(
                xmtpLogo,
                "XMTP logo",
                Modifier
                    .size(
                        64.dp,
                    ),
            )
            Text(
                "XMTP",
                style =
                    MaterialTheme.typography.displaySmall,
                color = ink,
                fontWeight =
                    FontWeight.Bold,
            )
        }
        Text(
            "Messenger",
            style =
                MaterialTheme.typography.headlineMedium,
        )
        OutlinedTextField(
            backend,
            {
                backend = it
            },
            label = {
                Text("Backend URL")
            },
            modifier =
                Modifier
                    .fillMaxWidth(),
            singleLine = true,
        )
        OutlinedTextField(
            credential,
            {
                credential = it
            },
            label = {
                Text("Credential (optional)")
            },
            visualTransformation =
                androidx.compose.ui.text.input
                    .PasswordVisualTransformation(),
            modifier =
                Modifier
                    .fillMaxWidth(),
            singleLine = true,
        )
        Row(
            verticalAlignment =
                Alignment.CenterVertically,
        ) {
            Checkbox(
                privateNetwork,
                {
                    privateNetwork = it
                },
            )
            Text("Allow local attachment network")
        }
        if (state.migrationRequired) {
            Text(
                "An old account is stored on this device. Its wallet key is unavailable. Reset only the selected local account to continue.",
            )
            state.migrationAccounts.forEach { id ->
                Action("Reset local account ${id.take(12)}") {
                    action(
                        MessengerAction
                            .ResetLegacyAccount(id),
                    )
                }
            }
        } else {
            Action(
                "Connect",
                !state
                    .busy &&
                    backend
                        .isNotBlank(),
            ) {
                action(
                    MessengerAction
                        .Connect(
                            backend,
                            credential,
                            privateNetwork,
                        ),
                )
            }
        }
        Text(
            "This app is for testing and debugging purposes only.",
            style =
                MaterialTheme.typography.bodySmall,
        )
        Spacer(
            Modifier
                .height(
                    24.dp,
                ),
        )
    }
}

@Composable private fun Conversations(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
) {
    val list = rememberLazyListState()
    LaunchedEffect(
        state.unknownTab,
    ) {
        snapshotFlow {
            list
                .firstVisibleItemIndex to
                list.layoutInfo.visibleItemsInfo.size
        }.distinctUntilChanged().collect {
            (
                index,
                count,
            ),
            ->
            if (count > 0) {
                action(
                    MessengerAction
                        .ListViewport(
                            index,
                            count,
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
                            MessengerAction
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
            if (state.conversations.none {
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
                state.conversations.filter {
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
                                MessengerAction
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
                            MessengerAction.LoadMoreConversations,
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

@Composable private fun Create(action: (MessengerAction) -> Unit) {
    var group by remember {
        mutableStateOf(false)
    }
    var recipients by remember {
        mutableStateOf("")
    }
    var name by remember {
        mutableStateOf("")
    }
    var description by remember {
        mutableStateOf("")
    }
    var admin by remember {
        mutableStateOf(false)
    }
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(
                20.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    12.dp,
                ),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(
                onClick = { group = false },
                modifier = Modifier.weight(1f).heightIn(min = 48.dp),
                colors =
                    ButtonDefaults.textButtonColors(
                        containerColor = if (!group) outgoing else Color(0xFFF1F3F7),
                        contentColor = if (!group) blue else ink,
                    ),
            ) { Text("Direct message") }
            TextButton(
                onClick = { group = true },
                modifier = Modifier.weight(1f).heightIn(min = 48.dp),
                colors =
                    ButtonDefaults.textButtonColors(
                        containerColor = if (group) outgoing else Color(0xFFF1F3F7),
                        contentColor = if (group) blue else ink,
                    ),
            ) { Text("Group") }
        }
        Text(
            if (group) "Group" else "Direct message",
            fontWeight =
                FontWeight.Bold,
        )
        OutlinedTextField(
            recipients,
            {
                recipients = it
            },
            label = {
                Text("Inbox IDs or Ethereum addresses, separated by commas")
            },
            modifier =
                Modifier
                    .fillMaxWidth(),
        )
        if (group) {
            OutlinedTextField(
                name,
                {
                    name = it
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
                    description = it
                },
                label = {
                    Text("Description")
                },
                modifier =
                    Modifier
                        .fillMaxWidth(),
            )
            Row(
                verticalAlignment =
                    Alignment.CenterVertically,
            ) {
                Checkbox(
                    admin,
                    {
                        admin = it
                    },
                )
                Text("Admins only")
            }
        }
        Action(
            "Create",
            recipients
                .isNotBlank(),
        ) {
            action(
                MessengerAction
                    .Create(
                        group,
                        recipients,
                        name,
                        description,
                        admin,
                    ),
            )
        }
    }
}

@Composable private fun Timeline(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
    composerExtra: @Composable () -> Unit,
    messageExtra: @Composable (MessageRow) -> Unit,
) {
    val list = rememberLazyListState()
    var text by remember(
        state.conversationId,
    ) {
        mutableStateOf("")
    }
    var selected by remember(
        state.conversationId,
    ) {
        mutableStateOf<MessageRow?>(null)
    }
    var emoji by remember {
        mutableStateOf("👍")
    }
    LaunchedEffect(
        state.conversationId,
        state.anchor?.messageId,
    ) {
        state.anchor?.let { anchor ->
            val index =
                state.messages.indexOfFirst {
                    it.id ==
                        anchor.messageId
                }
            if (index >= 0) {
                list
                    .scrollToItem(
                        index,
                        anchor.offsetPx,
                    )
            }
        }
    }
    LaunchedEffect(
        state.conversationId,
        state.messages,
    ) {
        snapshotFlow {
            list
                .firstVisibleItemIndex to
                list.firstVisibleItemScrollOffset
        }.distinctUntilChanged().collect {
            (
                index,
                offset,
            ),
            ->
            state.messages
                .getOrNull(index)
                ?.let {
                    action(
                        MessengerAction
                            .Viewport(
                                ScrollAnchor(
                                    it.id,
                                    it.sentAtNs,
                                    offset,
                                    index == 0 && offset == 0,
                                ),
                                index == 0 && offset == 0,
                            ),
                    )
                }
        }
    }
    Column(
        Modifier
            .fillMaxSize(),
    ) {
        state.readerError?.let {
            Notice(
                it,
                "Retry reader",
            ) {
                action(
                    MessengerAction.RetryReader,
                )
            }
        }
        state.historyNotice?.let {
            Notice(it)
        }
        if (state.conversationUnknown) {
            Row {
                TextButton(
                    {
                        action(
                            MessengerAction
                                .Consent(true),
                        )
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Allow")
                }
                TextButton(
                    {
                        action(
                            MessengerAction
                                .Consent(false),
                        )
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Block")
                }
            }
        }
        LazyColumn(
            state = list,
            reverseLayout = true,
            modifier =
                Modifier
                    .weight(1f),
            contentPadding =
                PaddingValues(
                    16.dp,
                ),
            verticalArrangement =
                Arrangement
                    .spacedBy(
                        8.dp,
                    ),
        ) {
            items(
                state.messages,
                key = {
                    it.id
                },
            ) { row ->
                Column(
                    Modifier
                        .fillMaxWidth(),
                    horizontalAlignment =
                        if (row.mine) {
                            Alignment.End
                        } else {
                            Alignment.Start
                        },
                ) {
                    Text(
                        "${row.sender} · ${row.day}",
                        style =
                            MaterialTheme.typography.labelSmall,
                    )
                    Column(
                        Modifier
                            .widthIn(
                                max =
                                    320.dp,
                            ).background(
                                if (row.mine) {
                                    outgoing
                                } else {
                                    Color(0xFFF1F3F7)
                                },
                                RoundedCornerShape(
                                    16.dp,
                                ),
                            ).clickable {
                                selected = row
                            }.padding(
                                12.dp,
                            ),
                    ) {
                        row.reply?.let {
                            Text(
                                "↳ $it",
                                color = blue,
                                style =
                                    MaterialTheme.typography.bodySmall,
                            )
                        }
                        Text(
                            row.text,
                        )
                        if (row.attachment) {
                            messageExtra(row)
                        }
                        Text(
                            "${row.time} · ${row.status}",
                            style =
                                MaterialTheme.typography.labelSmall,
                        )
                        Row {
                            row.reactions.forEach { reaction ->
                                TextButton(
                                    {
                                        action(
                                            MessengerAction
                                                .React(
                                                    row.id,
                                                    reaction.emoji,
                                                    reaction.mine,
                                                ),
                                        )
                                    },
                                    Modifier
                                        .heightIn(
                                            min =
                                                48.dp,
                                        ),
                                ) {
                                    Text("${reaction.emoji} ${reaction.count}")
                                }
                            }
                        }
                    }
                }
            }
            if (state.hasOlder) {
                item {
                    TextButton(
                        {
                            action(
                                MessengerAction.LoadOlder,
                            )
                        },
                        Modifier
                            .fillMaxWidth()
                            .heightIn(
                                min =
                                    48.dp,
                            ),
                    ) {
                        Text("Load older")
                    }
                }
            }
        }
        TextButton(
            {
                action(
                    MessengerAction.JumpToLatest,
                )
            },
            Modifier
                .fillMaxWidth()
                .heightIn(
                    min =
                        48.dp,
                ),
        ) {
            Text("Jump to latest")
        }
        state.replyPreview?.let {
            Notice(
                "Reply: $it",
                "Cancel",
            ) {
                action(
                    MessengerAction
                        .Reply(null),
                )
            }
        }
        composerExtra()
        Row(
            Modifier
                .fillMaxWidth()
                .padding(
                    12.dp,
                ),
            verticalAlignment =
                Alignment.CenterVertically,
        ) {
            if (state.features.attachments) {
                CircleAction(
                    "Select file",
                    AppIcons.Add,
                ) {
                    action(
                        MessengerAction
                            .Feature("select-file"),
                    )
                }
            }
            OutlinedTextField(
                text,
                {
                    text = it
                },
                placeholder = {
                    Text("Message")
                },
                modifier =
                    Modifier
                        .weight(1f),
                maxLines = 5,
                shape = RoundedCornerShape(28.dp),
                colors =
                    OutlinedTextFieldDefaults.colors(
                        unfocusedContainerColor = Color(0xFFF1F3F7),
                        focusedContainerColor = Color(0xFFF1F3F7),
                        unfocusedBorderColor = Color.Transparent,
                    ),
            )
            TextButton(
                {
                    action(
                        MessengerAction
                            .SendText(text),
                    )
                    text = ""
                },
                Modifier
                    .heightIn(
                        min =
                            48.dp,
                    ),
                enabled =
                    text
                        .isNotBlank() &&
                        !state.busy,
            ) {
                Text("Send")
            }
        }
    }
    selected?.let { row ->
        AlertDialog(
            onDismissRequest = {
                selected = null
            },
            title = {
                Text("Message")
            },
            text = {
                Column {
                    Text(
                        row.text,
                    )
                    if (!row.deleted) {
                        TextButton({
                            action(
                                MessengerAction
                                    .Reply(
                                        row.id,
                                    ),
                            )
                            selected = null
                        }) {
                            Text("Reply")
                        }
                        OutlinedTextField(
                            emoji,
                            {
                                emoji = it
                            },
                            label = {
                                Text("Emoji")
                            },
                        )
                        TextButton({
                            action(
                                MessengerAction
                                    .React(
                                        row.id,
                                        emoji,
                                        false,
                                    ),
                            )
                            selected = null
                        }) {
                            Text("React")
                        }
                    }
                    if (row
                            .mine &&
                        !row.deleted
                    ) {
                        TextButton({
                            action(
                                MessengerAction
                                    .DeleteMessage(
                                        row.id,
                                    ),
                            )
                            selected = null
                        }) {
                            Text("Delete message")
                        }
                    }
                    if (row.status == "Failed" || row.status == "Queued") {
                        TextButton({
                            action(
                                MessengerAction
                                    .RetrySend(
                                        row.id,
                                    ),
                            )
                            selected = null
                        }) {
                            Text("Retry publication")
                        }
                    }
                    if (row
                            .attachment &&
                        state.features.attachments
                    ) {
                        TextButton({
                            action(
                                MessengerAction
                                    .Feature(
                                        "open-file",
                                        row.id,
                                    ),
                            )
                            selected = null
                        }) {
                            Text("Download / Open")
                        }
                    }
                }
            },
            confirmButton = {
                TextButton({
                    selected = null
                }) {
                    Text("Close")
                }
            },
        )
    }
}

@Composable private fun Settings(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
) {
    var name by remember(
        state.settings.title,
    ) {
        mutableStateOf(
            state.settings.title,
        )
    }
    var description by remember(
        state.settings.description,
    ) {
        mutableStateOf(
            state.settings.description,
        )
    }
    var member by remember {
        mutableStateOf("")
    }
    var seconds by remember(
        state.settings.disappearingSeconds,
    ) {
        mutableStateOf(
            state.settings.disappearingSeconds,
        )
    }
    LazyColumn(
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
                            name = it
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
                            description = it
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
                            MessengerAction
                                .UpdateGroup(
                                    name,
                                    description,
                                ),
                        )
                    }
                    Text("Permissions: ${state.settings.preset}")
                    Action("All members") {
                        action(
                            MessengerAction
                                .SetPreset(false),
                        )
                    }
                    Action("Admins only") {
                        action(
                            MessengerAction
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
                        seconds = it
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
                                    MessengerAction
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
            Column {
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
                                    MessengerAction
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
                                    MessengerAction
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
                            member = it
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
                            MessengerAction
                                .AddMember(member),
                        )
                    }
                }
            }
        }
        if (state.features.metadata) {
            item {
                Column {
                    if (state.settings.group) {
                        Action("Group fields") {
                            action(
                                MessengerAction
                                    .Navigate(
                                        Screen.GROUP_FIELDS,
                                    ),
                            )
                        }
                    }
                    Action("My fields") {
                        action(
                            MessengerAction
                                .Navigate(
                                    Screen.MY_FIELDS,
                                ),
                        )
                    }
                }
            }
        }
        if (state.features.notifications) {
            item {
                Action("Notifications") {
                    action(
                        MessengerAction
                            .Feature("conversation-notifications"),
                    )
                }
            }
        }
        item {
            Action("Block") {
                action(
                    MessengerAction
                        .Consent(false),
                )
            }
        }
        if (state.settings.canRequestRemoval) {
            item {
                Action("Request removal") {
                    action(
                        MessengerAction.RequestRemoval,
                    )
                }
            }
        }
    }
}

@Composable private fun AppSettings(
    state: MessengerState,
    action: (MessengerAction) -> Unit,
) {
    var reset by remember {
        mutableStateOf(false)
    }
    LazyColumn(
        contentPadding =
            PaddingValues(
                20.dp,
            ),
        verticalArrangement =
            Arrangement
                .spacedBy(
                    16.dp,
                ),
    ) {
        item {
            Text("Backend", fontWeight = FontWeight.Bold)
            Text(
                state.backend,
            )
            Text("Inbox ID", fontWeight = FontWeight.Bold)
            Text(
                state.inbox,
            )
        }
        item {
            Text("Unread counts use local insertion time. Equal timestamps and imports can change these counts.")
        }
        if (state.features.notifications) {
            item {
                Action("Notifications") {
                    action(
                        MessengerAction
                            .Feature("app-notifications"),
                    )
                }
            }
        }
        if (state.features.attachments) {
            item {
                Action("Draft recovery") {
                    action(
                        MessengerAction
                            .Navigate(
                                Screen.DRAFTS,
                            ),
                    )
                }
            }
        }
        items(
            state.unknownSends,
            key = {
                it.draftId
            },
        ) { draft ->
            Column {
                Notice("Send outcome unknown. The original message may already exist.")
                Action("View chat") {
                    action(
                        MessengerAction
                            .OpenConversation(
                                draft.conversationId,
                            ),
                    )
                }
                TextButton(
                    {
                        action(
                            MessengerAction
                                .DiscardUnknownSend(
                                    draft.draftId,
                                ),
                        )
                    },
                    Modifier
                        .heightIn(
                            min =
                                48.dp,
                        ),
                ) {
                    Text("Discard record")
                }
            }
        }
        item {
            Action("Sign out") {
                action(
                    MessengerAction.SignOut,
                )
            }
        }
        item {
            Action("Delete my account") {
                reset = true
            }
        }
        item {
            Text("This app is for testing and debugging purposes only.")
        }
    }
    if (reset) {
        AlertDialog(
            onDismissRequest = {
                reset = false
            },
            title = {
                Text("Delete local account?")
            },
            text = {
                Text("This removes this backend's identity, SDK database and local files from this device.")
            },
            confirmButton = {
                TextButton({
                    reset = false
                    action(
                        MessengerAction.DeleteAccount,
                    )
                }) {
                    Text("Delete")
                }
            },
            dismissButton = {
                TextButton({
                    reset = false
                }) {
                    Text("Cancel")
                }
            },
        )
    }
}

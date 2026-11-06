package org.xmtp.android.example

import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.BackEventCompat
import androidx.activity.ComponentActivity
import androidx.activity.compose.PredictiveBackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.*
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.messenger.ReactionPickerHost
import org.xmtp.android.example.messenger.app.DestinationState
import org.xmtp.android.example.messenger.attachments.AttachmentHost
import org.xmtp.android.example.messenger.platform.systemReducedMotion
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.MessengerScreens
import org.xmtp.android.example.shared.Screen
import org.xmtp.android.example.shared.app.BackEdge
import org.xmtp.android.example.shared.app.BackPreview
import org.xmtp.android.example.shared.app.LocalBackPreview
import org.xmtp.android.example.shared.feature.chat.MessageOverlayState
import org.xmtp.android.example.shared.feature.conversations.CreateStep
import org.xmtp.android.example.shared.metadata.MetadataScreen

class MainActivity : ComponentActivity() {
    private val model: MessengerViewModel by viewModels {
        viewModelFactory {
            initializer { MessengerViewModel((application as ExampleApp).graph, createSavedStateHandle()) }
        }
    }
    internal lateinit var attachments: AttachmentHost
        private set

    private val notificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            model.graph.notificationPermission.complete(granted)
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        attachments = AttachmentHost(this, model.attachments)
        lifecycleScope.launch {
            model.notifications.permissionRequest.collect { request ->
                requestNotificationPermission(request)
            }
        }
        lifecycleScope.launch { model.openPush(intent) }
        setContent {
            val current = model.state.collectAsStateWithLifecycle().value
            val state = current.copy(shellState = current.shellState?.copy(reducedMotion = systemReducedMotion()))
            val destination =
                model.graph.destinations.state
                    .collectAsStateWithLifecycle()
                    .value
            val attachmentChoice =
                model.graph.attachments.choice
                    .collectAsStateWithLifecycle()
                    .value
            val shownOrigin =
                BackOrigin(
                    destination,
                    state.chatState?.overlay,
                    state.createState?.step?.takeIf { destination.route == AppRoute.CreateConversation },
                    attachmentChoice?.instanceId,
                )
            var gesture by remember { mutableStateOf<BackGesture?>(null) }
            DisposableEffect(Unit) {
                onDispose { gesture = null }
            }
            PredictiveBackHandler(
                enabled =
                    (
                        destination.route != AppRoute.Connect && destination.route != AppRoute.Chats &&
                            destination.route != AppRoute.ChooseHandle
                    ) ||
                        state.chatState?.overlay != null || attachmentChoice != null,
            ) { progress ->
                val origin = backOrigin()
                val identity = Any()
                try {
                    progress.collect { event ->
                        gesture =
                            BackGesture(
                                identity,
                                origin,
                                BackPreview(
                                    event.progress.coerceIn(0f, 1f),
                                    if (event.swipeEdge == BackEventCompat.EDGE_LEFT) BackEdge.LEFT else BackEdge.RIGHT,
                                ),
                            )
                    }
                    // A gesture cannot dismiss a surface that replaced its original target.
                    model.graph.session.withStateLock {
                        if (backOrigin() == origin) model.graph.back()
                    }
                } finally {
                    // Cancellation and disposal clear display state without applying Back.
                    if (gesture?.identity === identity) gesture = null
                }
            }
            val previewAllowed = shownOrigin.overlay == null && shownOrigin.attachmentChoiceId == null
            val preview =
                gesture
                    ?.takeIf {
                        it.origin == shownOrigin && previewAllowed
                    }?.preview
            CompositionLocalProvider(LocalBackPreview provides preview) {
                ReactionPickerHost(state, model::dispatch) { action ->
                    MessengerScreens(
                        state,
                        action,
                        extraScreen = { screen ->
                            when (screen) {
                                Screen.DRAFTS -> {
                                    attachments.Recovery(state.conversations)
                                }

                                Screen.GROUP_FIELDS, Screen.MY_FIELDS -> {
                                    key(destination.token) {
                                        MetadataScreen(
                                            model.metadataState.collectAsStateWithLifecycle().value,
                                            screen == Screen.MY_FIELDS,
                                            model.group::editMetadata,
                                        )
                                    }
                                }

                                else -> {
                                    Unit
                                }
                            }
                        },
                        composerExtra = { attachments.Composer(state.conversationId) },
                        messageExtra = { row ->
                            attachments.Message(
                                row,
                                state.conversationId,
                                if (state.screen == Screen.TIMELINE) {
                                    {
                                        model.chat.dispatch(
                                            org.xmtp.android.example.shared.feature.chat.ChatAction
                                                .SelectMessage(row.id),
                                        )
                                    }
                                } else {
                                    null
                                },
                            )
                        },
                        attachmentActions = { row, enabled, dismiss -> attachments.Actions(row, enabled, dismiss) },
                    )
                }
            }
        }
    }

    private fun backOrigin(): BackOrigin =
        model.graph.session.withStateLock {
            val destination = model.graph.destinations.state.value
            val createStep = model.graph.createConversation.state.value.step
            BackOrigin(
                destination,
                model.graph.chat.state.value.overlay,
                createStep.takeIf { destination.route == AppRoute.CreateConversation },
                model.graph.attachments.choice.value
                    ?.instanceId,
            )
        }

    private data class BackOrigin(
        val destination: DestinationState,
        val overlay: MessageOverlayState?,
        val createStep: CreateStep?,
        val attachmentChoiceId: Long?,
    )

    private data class BackGesture(
        val identity: Any,
        val origin: BackOrigin,
        val preview: BackPreview,
    )

    private fun requestNotificationPermission(request: Long) {
        if (request <= 0 || !model.notifications.configured || Build.VERSION.SDK_INT < 33) return
        val identity = model.graph.notificationPermission.claim(request) ?: return
        try {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        } catch (_: IllegalStateException) {
            model.graph.notificationPermission.launchFailed(identity)
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        lifecycleScope.launch { model.openPush(intent) }
    }

    override fun onStart() {
        super.onStart()
        model.visible(true)
    }

    override fun onResume() {
        super.onResume()
        model.foreground(true)
    }

    override fun onPause() {
        model.foreground(false)
        super.onPause()
    }

    override fun onStop() {
        model.visible(false)
        super.onStop()
    }

    override fun onDestroy() {
        attachments.close()
        super.onDestroy()
    }
}

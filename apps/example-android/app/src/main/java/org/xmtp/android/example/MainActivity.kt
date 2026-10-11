package org.xmtp.android.example

import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.compose.runtime.key
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import kotlinx.coroutines.launch
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.messenger.ReactionPickerHost
import org.xmtp.android.example.messenger.attachments.AttachmentHost
import org.xmtp.android.example.messenger.platform.systemReducedMotion
import org.xmtp.android.example.shared.AppRoute
import org.xmtp.android.example.shared.MessengerScreens
import org.xmtp.android.example.shared.Screen
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
            BackHandler(
                enabled =
                    (destination.route != AppRoute.Connect && destination.route != AppRoute.Chats) ||
                        state.chatState?.overlay != null,
            ) {
                model.graph.back()
            }
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
                    messageExtra = { row -> attachments.Message(row, state.conversationId) },
                )
            }
        }
    }

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

    override fun onResume() {
        super.onResume()
        model.foreground(true)
    }

    override fun onPause() {
        model.foreground(false)
        super.onPause()
    }

    override fun onDestroy() {
        attachments.close()
        super.onDestroy()
    }
}

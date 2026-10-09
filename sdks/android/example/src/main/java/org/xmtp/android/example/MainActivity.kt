package org.xmtp.android.example

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.messenger.ReactionPickerHost
import org.xmtp.android.example.messenger.attachments.AttachmentHost
import org.xmtp.android.example.shared.MessengerScreens
import org.xmtp.android.example.shared.Screen

class MainActivity : ComponentActivity() {
    private val model: MessengerViewModel by viewModels()
    private lateinit var attachments: AttachmentHost

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        attachments = AttachmentHost(this, model)
        setContent {
            val state = model.state.collectAsStateWithLifecycle().value
            ReactionPickerHost(state, model::dispatch) { action ->
                MessengerScreens(
                    state,
                    action,
                    extraScreen = { screen -> if (screen == Screen.DRAFTS) attachments.Recovery() },
                    composerExtra = { attachments.Composer() },
                    messageExtra = { row -> attachments.Message(row) },
                )
            }
        }
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

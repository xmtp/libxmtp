package org.xmtp.android.example

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.viewModels
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import org.xmtp.android.example.messenger.MessengerViewModel
import org.xmtp.android.example.messenger.ReactionPickerHost
import org.xmtp.android.example.shared.MessengerScreens

class MainActivity : ComponentActivity() {
    private val model: MessengerViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            val state = model.state.collectAsStateWithLifecycle().value
            ReactionPickerHost(state, model::dispatch) { action -> MessengerScreens(state, action) }
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
}

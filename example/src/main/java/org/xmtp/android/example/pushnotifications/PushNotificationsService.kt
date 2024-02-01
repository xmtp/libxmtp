package org.xmtp.android.example.pushnotifications

import android.Manifest
import android.app.PendingIntent
import android.content.pm.PackageManager
import android.util.Base64
import android.util.Log
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.R
import org.xmtp.android.example.conversation.ConversationDetailActivity
import org.xmtp.android.example.extension.truncatedAddress
import org.xmtp.android.example.utils.KeyUtil
import org.xmtp.android.library.messages.EnvelopeBuilder
import java.util.Date

class PushNotificationsService : FirebaseMessagingService() {

    companion object {
        private const val TAG = "PushNotificationService"

        internal const val CHANNEL_ID = "xmtp_direct_message"
    }

    override fun onNewToken(token: String) {
        super.onNewToken(token)
        PushNotificationTokenManager.syncPushNotificationsToken(token)
    }

    override fun onMessageReceived(remoteMessage: RemoteMessage) {
        super.onMessageReceived(remoteMessage)
        Log.d(TAG, "On message received.")

        val keysData = KeyUtil(this).loadKeys()
        if (keysData == null) {
            Log.e(TAG, "Attempting to send push to a logged out user.")
            return
        }

        val encryptedMessage = remoteMessage.data["encryptedMessage"]
        val topic = remoteMessage.data["topic"]
        val encryptedMessageData = Base64.decode(encryptedMessage, Base64.NO_WRAP)
        if (encryptedMessage == null || topic == null || encryptedMessageData == null) {
            Log.e(TAG, "Did not get correct message data from push")
            return
        }

        GlobalScope.launch(Dispatchers.Main) {
            ClientManager.createClient(keysData, applicationContext)
        }
        val conversation = ClientManager.client.fetchConversation(topic, includeGroups = true)
        if (conversation == null) {
            Log.e(TAG, "No keys or conversation persisted")
            return
        }
        val envelope = EnvelopeBuilder.buildFromString(topic, Date(), encryptedMessageData)
        val peerAddress = conversation.peerAddress
        val decodedMessage = conversation.decode(envelope)

        val body = decodedMessage.body
        val title = peerAddress.truncatedAddress()

        val pendingIntent = PendingIntent.getActivity(
            this,
            0,
            ConversationDetailActivity.intent(
                this,
                topic = topic,
                peerAddress = peerAddress
            ),
            (PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        )

        val builder = NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_xmtp_white)
            .setContentTitle(title)
            .setContentText(body)
            .setAutoCancel(true)
            .setColor(ContextCompat.getColor(this, R.color.black))
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setContentIntent(pendingIntent)

        // Use the URL as the ID for now until one is passed back from the server.
        NotificationManagerCompat.from(this).apply {
            if (ActivityCompat.checkSelfPermission(
                    applicationContext,
                    Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED
            ) {
                // TODO: Consider calling
                //    ActivityCompat#requestPermissions
                // here to request the missing permissions, and then overriding
                //   public void onRequestPermissionsResult(int requestCode, String[] permissions,
                //                                          int[] grantResults)
                // to handle the case where the user grants the permission. See the documentation
                // for ActivityCompat#requestPermissions for more details.
                return
            }
            notify(topic.hashCode(), builder.build())
        }
    }
}

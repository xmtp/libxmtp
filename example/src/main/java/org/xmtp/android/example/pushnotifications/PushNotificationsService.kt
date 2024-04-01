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
import kotlinx.coroutines.runBlocking
import org.xmtp.android.example.ClientManager
import org.xmtp.android.example.R
import org.xmtp.android.example.conversation.ConversationDetailActivity
import org.xmtp.android.example.extension.truncatedAddress
import org.xmtp.android.example.utils.KeyUtil
import org.xmtp.android.library.Conversation
import org.xmtp.android.library.messages.EnvelopeBuilder
import org.xmtp.android.library.messages.Topic
import uniffi.xmtpv3.org.xmtp.android.library.codecs.GroupMembershipChanges
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
        val welcomeTopic = Topic.userWelcome(ClientManager.client.installationId).description
        val builder = if (welcomeTopic == topic) {
            val group = ClientManager.client.conversations.fromWelcome(encryptedMessageData)
            val pendingIntent = PendingIntent.getActivity(
                this,
                0,
                ConversationDetailActivity.intent(
                    this,
                    topic = group.topic,
                    peerAddress = Conversation.Group(group).peerAddress
                ),
                (PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
            )

            NotificationCompat.Builder(this, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_xmtp_white)
                .setContentTitle(Conversation.Group(group).peerAddress.truncatedAddress())
                .setContentText("New Group Chat")
                .setAutoCancel(true)
                .setColor(ContextCompat.getColor(this, R.color.black))
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .setStyle(NotificationCompat.BigTextStyle().bigText("New Group Chat"))
                .setContentIntent(pendingIntent)
        } else {
            val conversation =
                runBlocking { ClientManager.client.fetchConversation(topic, includeGroups = true) }
            if (conversation == null) {
                Log.e(TAG, topic)
                Log.e(TAG, "No keys or conversation persisted")
                return
            }
            val decodedMessage = if (conversation is Conversation.Group) {
                runBlocking { conversation.group.processMessage(encryptedMessageData).decode() }
            } else {
                val envelope = EnvelopeBuilder.buildFromString(topic, Date(), encryptedMessageData)
                conversation.decode(envelope)
            }
            val peerAddress = conversation.peerAddress

            val body: String = if (decodedMessage.content<Any>() is String) {
                decodedMessage.body
            } else if (decodedMessage.content<Any>() is GroupMembershipChanges) {
                val changes = decodedMessage.content() as? GroupMembershipChanges
                "Membership Changed ${
                    changes?.membersAddedList?.mapNotNull { it.accountAddress }.toString()
                }"
            } else {
                ""
            }
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

            NotificationCompat.Builder(this, CHANNEL_ID)
                .setSmallIcon(R.drawable.ic_xmtp_white)
                .setContentTitle(title)
                .setContentText(body)
                .setAutoCancel(true)
                .setColor(ContextCompat.getColor(this, R.color.black))
                .setPriority(NotificationCompat.PRIORITY_DEFAULT)
                .setStyle(NotificationCompat.BigTextStyle().bigText(body))
                .setContentIntent(pendingIntent)
        }
        // Use the URL as the ID for now until one is passed back from the server.
        NotificationManagerCompat.from(this).apply {
            if (ActivityCompat.checkSelfPermission(
                    applicationContext,
                    Manifest.permission.POST_NOTIFICATIONS
                ) != PackageManager.PERMISSION_GRANTED
            ) {
                Log.e(TAG, "No push notification permissions granted")
                return
            }
            notify(topic.hashCode(), builder.build())
        }
    }
}

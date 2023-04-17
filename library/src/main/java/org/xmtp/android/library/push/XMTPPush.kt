package org.xmtp.android.library.push

import io.grpc.Grpc
import io.grpc.InsecureChannelCredentials
import io.grpc.ManagedChannel
import org.xmtp.android.library.XMTPException
import java.util.UUID

class XMTPPush() {
    lateinit var installationId: String
    lateinit var context: android.content.Context
    var pushServer: String = ""

    constructor(
        context: android.content.Context,
        pushServer: String = "",
    ) : this() {
        this.context = context
        val id = PushPreferences.getInstallationId(context)
        if (id.isNullOrBlank()) {
            installationId = UUID.randomUUID().toString()
            PushPreferences.setInstallationId(context, installationId)
        } else {
            this.installationId = id
        }
        this.pushServer = pushServer
    }

    fun register(token: String) {
        if (pushServer == "") {
            throw XMTPException("No push server")
        }

        val request = Service.RegisterInstallationRequest.newBuilder().also { request ->
            request.installationId = installationId
            request.deliveryMechanism = request.deliveryMechanism.toBuilder().also {
                it.firebaseDeviceToken = token
            }.build()
        }.build()
        client.registerInstallation(request)
    }

    fun subscribe(topics: List<String>) {
        if (pushServer == "") {
            throw XMTPException("No push server")
        }
        val request = Service.SubscribeRequest.newBuilder().also { request ->
            request.installationId = installationId
            request.addAllTopics(topics)
        }.build()
        client.subscribe(request)
    }

    val client: NotificationsGrpc.NotificationsFutureStub
        get() {
            val protocolClient: ManagedChannel =
                Grpc.newChannelBuilder(
                    pushServer, InsecureChannelCredentials.create(),
                ).build()
            return NotificationsGrpc.newFutureStub(protocolClient)
        }
}

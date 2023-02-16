package org.xmtp.android.library

import io.grpc.Grpc
import io.grpc.InsecureChannelCredentials
import io.grpc.ManagedChannel
import io.grpc.Metadata
import io.grpc.TlsChannelCredentials
import org.xmtp.android.library.messages.Pagination
import org.xmtp.android.library.messages.Topic
import org.xmtp.proto.message.api.v1.MessageApiGrpcKt
import org.xmtp.proto.message.api.v1.MessageApiOuterClass
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.Cursor
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.Envelope
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.PublishRequest
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.PublishResponse
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryRequest
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.QueryResponse
import java.io.Closeable
import java.util.concurrent.TimeUnit

interface ApiClient {
    val environment: XMTPEnvironment
    fun setAuthToken(token: String)
    suspend fun queryStrings(
        topics: List<String>,
        pagination: Pagination? = null,
        cursor: Cursor? = null,
    ): QueryResponse

    suspend fun query(topics: List<Topic>, pagination: Pagination? = null): QueryResponse
    suspend fun envelopes(topics: List<String>, pagination: Pagination? = null): List<Envelope>
    suspend fun publish(envelopes: List<Envelope>): PublishResponse
}

data class GRPCApiClient(override val environment: XMTPEnvironment, val secure: Boolean = true) :
    ApiClient, Closeable {
    companion object {
        val AUTHORIZATION_HEADER_KEY: Metadata.Key<String> =
            Metadata.Key.of("authorization", Metadata.ASCII_STRING_MARSHALLER)

        val CLIENT_VERSION_HEADER_KEY: Metadata.Key<String> =
            Metadata.Key.of("X-Client-Version", Metadata.ASCII_STRING_MARSHALLER)

        val APP_VERSION_HEADER_KEY: Metadata.Key<String> =
            Metadata.Key.of("X-App-Version", Metadata.ASCII_STRING_MARSHALLER)
    }

    private val channel: ManagedChannel =
        Grpc.newChannelBuilderForAddress(
            environment.rawValue,
            5556,
            if (secure) {
                TlsChannelCredentials.create()
            } else {
                InsecureChannelCredentials.create()
            },
        ).build()

    private val client: MessageApiGrpcKt.MessageApiCoroutineStub =
        MessageApiGrpcKt.MessageApiCoroutineStub(channel)
    private var authToken: String? = null

    override fun setAuthToken(token: String) {
        authToken = token
    }

    override suspend fun queryStrings(
        topics: List<String>,
        pagination: Pagination?,
        cursor: Cursor?,
    ): QueryResponse {
        val request = QueryRequest.newBuilder()
            .addAllContentTopics(topics).also {
                if (pagination != null) {
                    it.pagingInfo = pagination.pagingInfo
                }
                if (pagination?.startTime != null) {
                    it.endTimeNs = pagination.startTime.time * 1_000_000
                    it.pagingInfoBuilder.direction =
                        MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING
                }
                if (pagination?.endTime != null) {
                    it.startTimeNs = pagination.endTime.time * 1_000_000
                    it.pagingInfoBuilder.direction =
                        MessageApiOuterClass.SortDirection.SORT_DIRECTION_DESCENDING
                }
                if (cursor != null) {
                    it.pagingInfoBuilder.cursor = cursor
                }
            }.build()

        val headers = Metadata()

        authToken?.let { token ->
            headers.put(AUTHORIZATION_HEADER_KEY, "Bearer $token")
        }
        return client.query(request, headers = headers)
    }

    override suspend fun envelopes(topics: List<String>, pagination: Pagination?): List<Envelope> {
        val envelopes: MutableList<Envelope> = mutableListOf()
        var hasNextPage = true
        var cursor: Cursor? = null
        while (hasNextPage) {
            val response = queryStrings(topics = topics, pagination = pagination, cursor = cursor)
            envelopes.addAll(response.envelopesList)
            cursor = response.pagingInfo.cursor
            hasNextPage = response.envelopesList.isNotEmpty() && response.pagingInfo.hasCursor()
        }
        return envelopes
    }

    override suspend fun query(topics: List<Topic>, pagination: Pagination?): QueryResponse {
        return queryStrings(topics.map { it.description }, pagination)
    }

    override suspend fun publish(envelopes: List<Envelope>): PublishResponse {
        val request = PublishRequest.newBuilder().addAllEnvelopes(envelopes).build()
        val headers = Metadata()

        authToken?.let { token ->
            headers.put(AUTHORIZATION_HEADER_KEY, "Bearer $token")
        }

        headers.put(CLIENT_VERSION_HEADER_KEY, Constants.VERSION)
        headers.put(APP_VERSION_HEADER_KEY, Constants.VERSION)

        return client.publish(request, headers)
    }

    override fun close() {
        channel.shutdown().awaitTermination(5, TimeUnit.SECONDS)
    }
}

package org.xmtp.android.example.messenger
import uniffi.xmtp_sdk.*

internal suspend fun pendingMessagePage(
    before: Long?,
    read: suspend (ListMessagesOptions) -> List<Message>,
    count: suspend (ListMessagesOptions) -> ULong,
): BucketPage<Message> {
    val statuses = listOf(DeliveryStatus.UNPUBLISHED, DeliveryStatus.FAILED)
    return TimestampBuckets<Message>({ it.sentAt.ns }, maxRows = 50).load(
        before,
        read = { upper, limit ->
            statuses
                .flatMap { status ->
                    read(
                        publishedSelection().copy(
                            deliveryStatus = status,
                            sentBefore = upper?.let(::Timestamp),
                            limit = limit.toUInt(),
                        ),
                    )
                }.sortedByDescending { it.sentAt.ns }
                .take(limit)
        },
        count = { upper, lower ->
            statuses.fold(0uL) { total, status ->
                total +
                    count(
                        publishedSelection().copy(
                            deliveryStatus = status,
                            sentBefore = upper?.let(::Timestamp),
                            sentAfter = lower?.let(::Timestamp),
                        ),
                    )
            }
        },
    )
}

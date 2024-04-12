package org.xmtp.android.library.messages

import org.xmtp.proto.message.api.v1.MessageApiOuterClass.Cursor
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.SortDirection
import java.util.Date

typealias PagingInfo = org.xmtp.proto.message.api.v1.MessageApiOuterClass.PagingInfo
typealias PagingInfoCursor = Cursor
typealias PagingInfoSortDirection = SortDirection

data class Pagination(
    val limit: Int? = null,
    val direction: PagingInfoSortDirection? = SortDirection.SORT_DIRECTION_DESCENDING,
    val before: Date? = null,
    val after: Date? = null,
) {
    val pagingInfo: PagingInfo
        get() {
            return PagingInfo.newBuilder().also { page ->
                limit?.let {
                    page.limit = it
                }
                if (direction != null) {
                    page.direction = direction
                }
            }.build()
        }
}

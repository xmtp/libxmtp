package org.xmtp.android.library.messages

import org.xmtp.proto.message.api.v1.MessageApiOuterClass.Cursor
import org.xmtp.proto.message.api.v1.MessageApiOuterClass.SortDirection
import java.util.Date

typealias PagingInfo = org.xmtp.proto.message.api.v1.MessageApiOuterClass.PagingInfo
typealias PagingInfoCursor = Cursor
typealias PagingInfoSortDirection = SortDirection

data class Pagination(
    val limit: Int? = null,
    val direction: PagingInfoSortDirection? = null,
    val before: Date? = null,
    val after: Date? = null,
) {
    val pagingInfo: PagingInfo
        get() {
            return PagingInfo.newBuilder().also {
                if (limit != null) {
                    it.limit = limit
                }
                if (direction != null) {
                    it.direction = direction
                }
            }.build()
        }
}

class PagingInfoBuilder {
    companion object {
        fun buildFromPagingInfo(
            limit: Int? = null,
            cursor: PagingInfoCursor? = null,
            direction: PagingInfoSortDirection? = null,
        ): PagingInfo {
            return PagingInfo.newBuilder().also {
                if (limit != null) {
                    it.limit = limit
                }
                if (cursor != null) {
                    it.cursor = cursor
                }
                if (direction != null) {
                    it.direction = direction
                }
            }.build()
        }
    }
}

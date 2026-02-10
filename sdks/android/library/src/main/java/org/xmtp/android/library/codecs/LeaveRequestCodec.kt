package org.xmtp.android.library.codecs

/**
 * Represents a leave request message sent when a user wants to leave a group.
 *
 * Leave requests are automatically sent when calling `leaveGroup()` on a conversation.
 * Users should not need to manually encode or send this content type.
 *
 * Following protobuf semantics, empty ByteArray is treated as equivalent to null during encoding/decoding.
 *
 * @property authenticatedNote Optional authenticated note for the leave request
 */
data class LeaveRequest(
    val authenticatedNote: ByteArray? = null,
) {
    override fun equals(other: Any?): Boolean {
        if (this === other) return true
        if (javaClass != other?.javaClass) return false

        other as LeaveRequest

        return authenticatedNote.contentEquals(other.authenticatedNote)
    }

    override fun hashCode(): Int = authenticatedNote?.contentHashCode() ?: 0

    companion object {
        fun create(authenticatedNote: ByteArray? = null): LeaveRequest =
            LeaveRequest(
                authenticatedNote = if (authenticatedNote?.isEmpty() == true) null else authenticatedNote,
            )
    }
}

val ContentTypeLeaveRequest =
    ContentTypeIdBuilder.builderFromAuthorityId(
        "xmtp.org",
        "leave_request",
        versionMajor = 1,
        versionMinor = 0,
    )

data class LeaveRequestCodec(
    override var contentType: ContentTypeId = ContentTypeLeaveRequest,
) : ContentCodec<LeaveRequest> {
    override fun encode(content: LeaveRequest): EncodedContent {
        val ffi =
            uniffi.xmtpv3.FfiLeaveRequest(
                authenticatedNote = content.authenticatedNote,
            )

        return EncodedContent.parseFrom(
            uniffi.xmtpv3.encodeLeaveRequest(ffi),
        )
    }

    override fun decode(content: EncodedContent): LeaveRequest {
        val decoded = uniffi.xmtpv3.decodeLeaveRequest(content.toByteArray())

        return LeaveRequest.create(
            authenticatedNote = decoded.authenticatedNote,
        )
    }

    override fun fallback(content: LeaveRequest): String = "A member has requested leaving the group"

    override fun shouldPush(content: LeaveRequest): Boolean = false
}

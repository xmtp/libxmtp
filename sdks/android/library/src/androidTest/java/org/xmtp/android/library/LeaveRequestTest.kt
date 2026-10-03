package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import uniffi.xmtp_sdk.*

class LeaveRequestTest : BaseInstrumentedTest() {
    private suspend fun roundTrip(note: ByteArray?): LeaveRequest {
        val fixtures = createFixtures()
        val dm = fixtures.alixClient.conversations().createDm(fixtures.boClient.inboxId())
        val id = dm.send(LeaveRequestCodec(), LeaveRequest(note))
        return (
            (
                dm
                    .messages()
                    .single {
                        it.id == id
                    }.content as SDKMessageContent.Standard
            ).value as MessageContent.LeaveRequest
        ).v1
    }

    @Test fun testCanUseLeaveRequestCodec() =
        runBlocking {
            assertArrayEquals(
                "random_auth_note".toByteArray(),
                roundTrip("random_auth_note".toByteArray()).authenticatedNote,
            )
        }

    @Test fun testCanUseLeaveRequestCodecWithNilNote() = runBlocking { assertNull(roundTrip(null).authenticatedNote) }

    @Test fun testLeaveRequestCodecEncodeDecode() {
        val value = LeaveRequest("test note".toByteArray())
        val codec = LeaveRequestCodec()
        assertArrayEquals(value.authenticatedNote, codec.decode(codec.encode(value)).authenticatedNote)
    }

    @Test fun testLeaveRequestCodecEncodeDecodeWithNilNote() {
        val codec = LeaveRequestCodec()
        assertNull(codec.decode(codec.encode(LeaveRequest(null))).authenticatedNote)
    }

    @Test fun testLeaveRequestCodecFallback() =
        assertEquals("A member has requested leaving the group", LeaveRequestCodec().fallback(LeaveRequest(null)))

    @Test fun testLeaveRequestCodecShouldPush() = assertFalse(LeaveRequestCodec().shouldPush(LeaveRequest(null)))

    @Test fun testLeaveRequestCodecContentType() =
        assertEquals(ContentTypeId("xmtp.org", "leaveRequest", 1u, 0u), LeaveRequestCodec().type)

    @Test fun testLeaveRequestCreateNormalizesEmptyByteArray() {
        val codec = LeaveRequestCodec()
        assertNull(codec.decode(codec.encode(LeaveRequest(byteArrayOf()))).authenticatedNote)
        assertArrayEquals(
            "note".toByteArray(),
            codec.decode(codec.encode(LeaveRequest("note".toByteArray()))).authenticatedNote,
        )
        assertNull(codec.decode(codec.encode(LeaveRequest(null))).authenticatedNote)
    }
}

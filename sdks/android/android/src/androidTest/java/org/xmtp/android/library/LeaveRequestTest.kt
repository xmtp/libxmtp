package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.codecs.ContentTypeLeaveRequest
import org.xmtp.android.library.codecs.LeaveRequest
import org.xmtp.android.library.codecs.LeaveRequestCodec

@RunWith(AndroidJUnit4::class)
class LeaveRequestTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: Client
    private lateinit var boClient: Client

    @org.junit.Before
    override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
    }

    @Test
    fun testCanUseLeaveRequestCodec() {
        Client.register(codec = LeaveRequestCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        val leaveRequest =
            LeaveRequest(
                authenticatedNote = "random_auth_note".toByteArray(),
            )

        runBlocking {
            alixConversation.send(
                content = leaveRequest,
                options = SendOptions(contentType = ContentTypeLeaveRequest),
            )
        }

        val messages = runBlocking { alixConversation.messages() }

        assertEquals(2, messages.size)

        if (messages.size == 2) {
            val content: LeaveRequest? = messages.first().content()
            assertNotNull(content)
            assertArrayEquals("random_auth_note".toByteArray(), content?.authenticatedNote)
        }
    }

    @Test
    fun testCanUseLeaveRequestCodecWithNilNote() {
        Client.register(codec = LeaveRequestCodec())

        val alixConversation =
            runBlocking {
                alixClient.conversations.newConversation(boClient.inboxId)
            }

        val leaveRequest = LeaveRequest(authenticatedNote = null)

        runBlocking {
            alixConversation.send(
                content = leaveRequest,
                options = SendOptions(contentType = ContentTypeLeaveRequest),
            )
        }

        val messages = runBlocking { alixConversation.messages() }

        assertEquals(2, messages.size)

        if (messages.size == 2) {
            val content: LeaveRequest? = messages.first().content()
            assertNotNull(content)
            assertNull(content?.authenticatedNote)
        }
    }

    @Test
    fun testLeaveRequestCodecEncodeDecode() {
        val codec = LeaveRequestCodec()

        val original = LeaveRequest(authenticatedNote = "test note".toByteArray())
        val encoded = codec.encode(original)
        val decoded = codec.decode(encoded)

        assertEquals(original, decoded)
        assertArrayEquals(original.authenticatedNote, decoded.authenticatedNote)
    }

    @Test
    fun testLeaveRequestCodecEncodeDecodeWithNilNote() {
        val codec = LeaveRequestCodec()

        val original = LeaveRequest(authenticatedNote = null)
        val encoded = codec.encode(original)
        val decoded = codec.decode(encoded)

        assertEquals(original, decoded)
        assertNull(decoded.authenticatedNote)
    }

    @Test
    fun testLeaveRequestCodecFallback() {
        val codec = LeaveRequestCodec()
        val content = LeaveRequest()
        val fallback = codec.fallback(content)
        assertEquals("A member has requested leaving the group", fallback)
    }

    @Test
    fun testLeaveRequestCodecShouldPush() {
        val codec = LeaveRequestCodec()
        val content = LeaveRequest()
        val shouldPush = codec.shouldPush(content)
        assertFalse(shouldPush)
    }

    @Test
    fun testLeaveRequestCodecContentType() {
        val codec = LeaveRequestCodec()
        assertEquals(ContentTypeLeaveRequest, codec.contentType)
    }

    @Test
    fun testLeaveRequestCreateNormalizesEmptyByteArray() {
        val requestWithEmpty = LeaveRequest.create(authenticatedNote = byteArrayOf())
        assertNull(requestWithEmpty.authenticatedNote)

        val requestWithData = LeaveRequest.create(authenticatedNote = "note".toByteArray())
        assertNotNull(requestWithData.authenticatedNote)

        val requestWithNull = LeaveRequest.create(authenticatedNote = null)
        assertNull(requestWithNull.authenticatedNote)
    }
}

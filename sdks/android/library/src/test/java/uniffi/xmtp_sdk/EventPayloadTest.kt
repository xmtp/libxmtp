package uniffi.xmtp_sdk

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class EventPayloadTest {
    @Test
    fun emptyHmacPayloadRoundTripsThroughNativeBuffer() {
        val event = ClientEvent.HmacKeysUpdated(HmacKeysUpdated())
        val restored = FfiConverterTypeClientEvent.lift(FfiConverterTypeClientEvent.lower(event))
        assertEquals(event, restored)
        assertEquals(HmacKeysUpdated(), (restored as ClientEvent.HmacKeysUpdated).hmacKeysUpdated)
    }

    @Test
    fun namedConsentPayloadRoundTripsThroughNativeBuffer() {
        val payload = ConsentChanged(ConsentEntityKind.INBOX, "event-payload-inbox", EventConsentState.ALLOWED)
        val event = ClientEvent.ConsentChanged(payload)
        val restored = FfiConverterTypeClientEvent.lift(FfiConverterTypeClientEvent.lower(event))
        assertEquals(event, restored)
        assertEquals(payload, (restored as ClientEvent.ConsentChanged).consentChanged)
    }

    private val groupBytes = byteArrayOf(0, 1, -1, -128, 42)
    private val messageBytes = byteArrayOf(4, -1, 0, 9)
    private val installationBytes = byteArrayOf(-128, 0, 17, -1)

    private fun roundTrip(event: ClientEvent): ClientEvent =
        FfiConverterTypeClientEvent.lift(FfiConverterTypeClientEvent.lower(event))

    // verifies: EVENT-024
    @Test
    fun conversationJoinedKeepsNativeGroupBytes() {
        val payload = ConversationJoined(groupBytes, EventConversationType.GROUP, JoinOrigin.CREATED, "adder-inbox")
        val restored =
            (
                roundTrip(
                    ClientEvent.ConversationJoined(payload),
                ) as ClientEvent.ConversationJoined
            ).conversationJoined
        assertArrayEquals(groupBytes, restored.groupId)
        assertEquals(payload.conversationType, restored.conversationType)
        assertEquals(payload.origin, restored.origin)
        assertEquals(payload.adderInboxId, restored.adderInboxId)
    }

    // verifies: EVENT-024
    @Test
    fun metadataKeepsNativeGroupBytes() {
        val payload = MetadataChanged(groupBytes, listOf("name", "app_data"))
        val restored =
            (
                roundTrip(
                    ClientEvent.ConversationMetadataChanged(payload),
                ) as ClientEvent.ConversationMetadataChanged
            ).metadataChanged
        assertArrayEquals(groupBytes, restored.groupId)
        assertEquals(payload.changed, restored.changed)
    }

    // verifies: EVENT-024
    @Test
    fun receivedMessageKeepsNativeIdsAndThreeFieldContentType() {
        val contentType = EventContentTypeId("xmtp.org", "text", 1u)
        val payload = MessageReceived(groupBytes, messageBytes, contentType, "sender-inbox")
        val restored = (roundTrip(ClientEvent.MessageReceived(payload)) as ClientEvent.MessageReceived).messageReceived
        assertArrayEquals(groupBytes, restored.groupId)
        assertArrayEquals(messageBytes, restored.messageId)
        assertEquals(contentType, restored.contentType)
        assertEquals(payload.senderInboxId, restored.senderInboxId)
    }

    // verifies: EVENT-024
    @Test
    fun expiredMessageKeepsBothNativeIds() {
        val restored =
            (
                roundTrip(
                    ClientEvent.MessageExpired(MessageRef(groupBytes, messageBytes)),
                ) as ClientEvent.MessageExpired
            ).messageExpired
        assertArrayEquals(groupBytes, restored.groupId)
        assertArrayEquals(messageBytes, restored.messageId)
    }

    // verifies: EVENT-024
    @Test
    fun registeredIdentityKeepsNativeInstallationKey() {
        val restored =
            (
                roundTrip(
                    ClientEvent.IdentityRegistered(IdentityRegistered("identity-inbox", installationBytes)),
                ) as ClientEvent.IdentityRegistered
            ).identityRegistered
        assertEquals("identity-inbox", restored.inboxId)
        assertArrayEquals(installationBytes, restored.installationKey)
    }

    // verifies: EVENT-024
    @Test
    fun installationEventsKeepNativeKeyAndRevocationFlag() {
        val added =
            (
                roundTrip(
                    ClientEvent.IdentityOwnInstallationAdded(InstallationRef(installationBytes)),
                ) as ClientEvent.IdentityOwnInstallationAdded
            ).ownInstallationAdded
        val revoked =
            (
                roundTrip(
                    ClientEvent.IdentityOwnInstallationRevoked(InstallationRevoked(installationBytes, true)),
                ) as ClientEvent.IdentityOwnInstallationRevoked
            ).ownInstallationRevoked
        assertArrayEquals(installationBytes, added.installationKey)
        assertArrayEquals(installationBytes, revoked.installationKey)
        assertEquals(true, revoked.isThisInstallation)
    }

    // verifies: EVENT-024
    @Test
    fun eventFilterKeepsNativeGroupBytes() {
        val contentType = EventContentTypeId("xmtp.org", "text", 1u)
        val filter =
            EventFilter(listOf(EventKind.MESSAGE_RECEIVED), listOf(groupBytes, messageBytes), listOf(contentType), true)
        val restored = FfiConverterTypeEventFilter.lift(FfiConverterTypeEventFilter.lower(filter))
        assertEquals(filter.kinds, restored.kinds)
        assertEquals(2, restored.groupIds!!.size)
        assertArrayEquals(groupBytes, restored.groupIds!![0])
        assertArrayEquals(messageBytes, restored.groupIds!![1])
        assertEquals(filter.contentTypes, restored.contentTypes)
        assertEquals(true, restored.referencesOwnMessages)
    }
}

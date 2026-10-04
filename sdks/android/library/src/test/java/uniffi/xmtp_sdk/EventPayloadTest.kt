package uniffi.xmtp_sdk

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
}

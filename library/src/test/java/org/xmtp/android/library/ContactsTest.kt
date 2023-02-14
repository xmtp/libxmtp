package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.walletAddress

class ContactsTest {

    @Test
    fun testNormalizesAddresses() {
        val fixtures = fixtures()
        fixtures.bobClient.ensureUserContactPublished()
        val bobAddressLowercased = fixtures.bobClient.address?.lowercase()
        val bobContact = fixtures.aliceClient.getUserContact(peerAddress = bobAddressLowercased!!)
        assert(bobContact != null)
    }

    @Test
    fun testCanFindContact() {
        val fixtures = fixtures()
        fixtures.bobClient.ensureUserContactPublished()
        val contactBundle = fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress)
        assertEquals(contactBundle?.walletAddress, fixtures.bob.walletAddress)
    }

    @Test
    fun testCachesContacts() {
        val fixtures = fixtures()
        fixtures.bobClient.ensureUserContactPublished()
        // Look up the first time
        fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress)
        fixtures.fakeApiClient.assertNoQuery {
            val contactBundle = fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress)
            assertEquals(contactBundle?.walletAddress, fixtures.bob.walletAddress)
        }
        assert(fixtures.aliceClient.contacts.has(fixtures.bob.walletAddress))
    }
}

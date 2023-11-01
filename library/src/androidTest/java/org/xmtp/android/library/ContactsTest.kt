package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.walletAddress
@RunWith(AndroidJUnit4::class)
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

    @Test
    fun testAllowAddress() {
        val fixtures = fixtures()

        val contacts = fixtures.bobClient.contacts
        var result = contacts.isAllowed(fixtures.alice.walletAddress)

        assert(!result)

        contacts.allow(listOf(fixtures.alice.walletAddress))

        result = contacts.isAllowed(fixtures.alice.walletAddress)
        assert(result)
    }

    @Test
    fun testBlockAddress() {
        val fixtures = fixtures()

        val contacts = fixtures.bobClient.contacts
        var result = contacts.isAllowed(fixtures.alice.walletAddress)

        assert(!result)

        contacts.deny(listOf(fixtures.alice.walletAddress))

        result = contacts.isDenied(fixtures.alice.walletAddress)
        assert(result)
    }
}

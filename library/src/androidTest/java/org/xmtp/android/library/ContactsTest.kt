package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.messages.walletAddress

@RunWith(AndroidJUnit4::class)
class ContactsTest {

    @Test
    fun testNormalizesAddresses() {
        val fixtures = fixtures()
        runBlocking { fixtures.bobClient.ensureUserContactPublished() }
        val bobAddressLowerCased = fixtures.bobClient.address.lowercase()
        val bobContact = fixtures.aliceClient.getUserContact(peerAddress = bobAddressLowerCased)
        assert(bobContact != null)
    }

    @Test
    fun testCanFindContact() {
        val fixtures = fixtures()
        runBlocking { fixtures.bobClient.ensureUserContactPublished() }
        val contactBundle = fixtures.aliceClient.contacts.find(fixtures.bob.walletAddress)
        assertEquals(contactBundle?.walletAddress, fixtures.bob.walletAddress)
    }

    @Test
    fun testAllowAddress() {
        val fixtures = fixtures()

        val contacts = fixtures.bobClient.contacts
        var result = contacts.isAllowed(fixtures.alice.walletAddress)

        assert(!result)

        runBlocking { contacts.allow(listOf(fixtures.alice.walletAddress)) }

        result = contacts.isAllowed(fixtures.alice.walletAddress)
        assert(result)
    }

    @Test
    fun testDenyAddress() {
        val fixtures = fixtures()

        val contacts = fixtures.bobClient.contacts
        var result = contacts.isAllowed(fixtures.alice.walletAddress)

        assert(!result)

        runBlocking { contacts.deny(listOf(fixtures.alice.walletAddress)) }

        result = contacts.isDenied(fixtures.alice.walletAddress)
        assert(result)
    }
}

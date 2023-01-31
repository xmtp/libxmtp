package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.ContactBundleBuilder
import org.xmtp.android.library.messages.Topic

data class Contacts(
    var client: Client,
    var knownBundles: Map<String, ContactBundle> = mapOf(),
    var hasIntroduced: Map<String, Boolean> = mapOf()
) {

    fun has(peerAddress: String): Boolean =
        knownBundles[peerAddress] != null

    fun needsIntroduction(peerAddress: String): Boolean =
        hasIntroduced[peerAddress] != true

    fun find(peerAddress: String): ContactBundle? {
        val knownBundle = knownBundles[peerAddress]
        if (knownBundle != null) {
            return knownBundle
        }
        val response = runBlocking { client.query(topics = listOf(Topic.contact(peerAddress))) }
        for (envelope in response.envelopesList) {
            val contactBundle = ContactBundleBuilder.buildFromEnvelope(envelope)
            knownBundles.toMutableMap()[peerAddress] = contactBundle
            return contactBundle
        }
        return null
    }
}

package org.xmtp.android.library

import kotlinx.coroutines.runBlocking
import org.xmtp.android.library.messages.ContactBundle
import org.xmtp.android.library.messages.ContactBundleBuilder
import org.xmtp.android.library.messages.Topic

data class Contacts(
    var client: Client,
    val knownBundles: MutableMap<String, ContactBundle> = mutableMapOf(),
    val hasIntroduced: MutableMap<String, Boolean> = mutableMapOf()
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
        val response = runBlocking { client.query(topic = Topic.contact(peerAddress)) }

        if (response.envelopesList.isNullOrEmpty()) return null

        for (envelope in response.envelopesList) {
            knownBundles[peerAddress] = ContactBundleBuilder.buildFromEnvelope(envelope)
        }
        return ContactBundleBuilder.buildFromEnvelope(response.envelopesList.first())
    }
}

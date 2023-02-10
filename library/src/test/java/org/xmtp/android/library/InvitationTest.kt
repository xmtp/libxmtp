package org.xmtp.android.library

import org.junit.Assert.assertEquals
import org.junit.Test
import org.xmtp.android.library.messages.InvitationV1
import org.xmtp.android.library.messages.PrivateKeyBundleV1
import org.xmtp.android.library.messages.SealedInvitation
import org.xmtp.android.library.messages.SealedInvitationBuilder
import org.xmtp.android.library.messages.createRandom
import org.xmtp.android.library.messages.generate
import org.xmtp.android.library.messages.getInvitation
import org.xmtp.android.library.messages.getPublicKeyBundle
import org.xmtp.android.library.messages.header
import org.xmtp.android.library.messages.toV2
import java.util.Date

class InvitationTest {

    @Test
    fun testGenerateSealedInvitation() {
        val aliceWallet = FakeWallet.generate()
        val bobWallet = FakeWallet.generate()
        val alice = PrivateKeyBundleV1.newBuilder().build().generate(wallet = aliceWallet)
        val bob = PrivateKeyBundleV1.newBuilder().build().generate(wallet = bobWallet)
        val invitation = InvitationV1.newBuilder().build().createRandom()
        val newInvitation = SealedInvitationBuilder.buildFromV1(
            sender = alice.toV2(),
            recipient = bob.toV2().getPublicKeyBundle(),
            created = Date(),
            invitation = invitation
        )
        val deserialized = SealedInvitation.parseFrom(newInvitation.toByteArray())
        assert(!deserialized.v1.headerBytes.isEmpty)
        assertEquals(newInvitation, deserialized)
        val header = newInvitation.v1.header
        // Ensure the headers haven't been mangled
        assertEquals(header.sender, alice.toV2().getPublicKeyBundle())
        assertEquals(header.recipient, bob.toV2().getPublicKeyBundle())
        // Ensure alice can decrypt the invitation
        val aliceInvite = newInvitation.v1.getInvitation(viewer = alice.toV2())
        assertEquals(aliceInvite.topic, invitation.topic)
        assertEquals(
            aliceInvite.aes256GcmHkdfSha256.keyMaterial,
            invitation.aes256GcmHkdfSha256.keyMaterial
        )
        // Ensure bob can decrypt the invitation
        val bobInvite = newInvitation.v1.getInvitation(viewer = bob.toV2())
        assertEquals(bobInvite.topic, invitation.topic)
        assertEquals(
            bobInvite.aes256GcmHkdfSha256.keyMaterial,
            invitation.aes256GcmHkdfSha256.keyMaterial
        )
    }
}

package org.xmtp.android.example.messenger.notifications

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.util.Base64

class NotificationHandlerTest {
    private val groupA = "11".repeat(16)
    private val groupB = "22".repeat(16)
    private val installationA = "aa".repeat(32)
    private val installationB = "bb".repeat(32)

    private fun payload(
        kind: Int = 0,
        id: String = groupA,
        sequence: String = "1",
    ) = mapOf(
        "topic" to
            Base64.getEncoder().encodeToString(
                byteArrayOf(kind.toByte()) +
                    id
                        .chunked(2)
                        .map {
                            it.toInt(16).toByte()
                        }.toByteArray(),
            ),
        "sequence_id" to sequence,
    )

    private class Host(
        var owner: PushOwner?,
        var appEnabled: Boolean = true,
    ) : PushAdmission {
        val local = mutableMapOf<Pair<String, String>, PushConversation>()
        val posted = linkedMapOf<String, PushRoute>()
        var afterRead: (() -> Unit)? = null

        override fun current() = owner

        override suspend fun enabled(owner: PushOwner) = appEnabled && this.owner == owner

        override suspend fun conversation(
            owner: PushOwner,
            source: String,
        ): PushConversation? {
            val value = local[owner.profile to source]
            afterRead?.also {
                afterRead = null
                it()
            }
            return value
        }

        override fun postIfCurrent(
            owner: PushOwner,
            envelope: PushEnvelope,
            route: PushRoute,
        ): Boolean {
            if (this.owner != owner) return false
            posted[envelope.tag] = route
            return true
        }
    }

    @Test fun parsesUnsignedSequencesAndRejectsMalformedPayloads() {
        assertEquals(ULong.MAX_VALUE, parsePush(payload(sequence = "18446744073709551615"))?.sequence)
        assertEquals(9007199254740993uL, parsePush(payload(sequence = "9007199254740993"))?.sequence)
        for (sequence in listOf(
            "18446744073709551616",
            "-1",
            "+1",
            " 1",
            "1.0",
            "",
            "1e3",
        )) {
            assertNull(parsePush(payload(sequence = sequence)))
        }
        for (data in listOf(
            emptyMap(),
            payload(2, installationA),
            payload(4),
            payload(id = "11"),
            payload() + ("topic" to "!"),
            payload() + ("topic" to payload().getValue("topic").dropLast(1)),
        )) {
            assertNull(parsePush(data))
        }
    }

    @Test fun admitsKnownGroupAndDmAndDeduplicatesByTopicSequence() =
        runBlocking {
            val host = Host(PushOwner("A", 1, installationA))
            host.local["A" to groupA] = PushConversation(groupA, true, true)
            host.local["A" to groupB] = PushConversation("dm:logical-pair", true, true)
            val handler = PushHandler(host)
            assertTrue(handler.receive(payload()))
            assertTrue(handler.receive(payload()))
            assertEquals(1, host.posted.size)
            assertTrue(handler.receive(payload(sequence = "2")))
            assertEquals(2, host.posted.size)
            assertTrue(handler.receive(payload(id = groupB)))
            assertEquals(
                "dm:logical-pair",
                host.posted.values
                    .last()
                    .conversation,
            )
        }

    @Test fun dropsUnknownDeniedMutedAndDisabledConversations() =
        runBlocking {
            val host = Host(PushOwner("A", 1, installationA))
            val handler = PushHandler(host)
            assertFalse(handler.receive(payload()))
            for (value in listOf(PushConversation(groupA, false, true), PushConversation(groupA, true, false))) {
                host.local["A" to groupA] = value
                assertFalse(handler.receive(payload()))
            }
            host.local["A" to groupA] = PushConversation(groupA, true, true)
            host.appEnabled = false
            assertFalse(handler.receive(payload()))
            assertTrue(host.posted.isEmpty())
        }

    @Test fun welcomeRequiresTheCurrentInstallationAndRoutesToList() =
        runBlocking {
            val host = Host(PushOwner("A", 1, installationA))
            val handler = PushHandler(host)
            assertFalse(handler.receive(payload(1, installationB)))
            assertTrue(handler.receive(payload(1, installationA)))
            assertNull(
                host.posted.values
                    .single()
                    .conversation,
            )
        }

    @Test fun rejectsLateReadsAndFreshConsentChanges() =
        runBlocking {
            val host = Host(PushOwner("A", 1, installationA))
            host.local["A" to groupA] = PushConversation(groupA, true, true)
            val handler = PushHandler(host)
            host.afterRead = { host.owner = PushOwner("A", 2, installationA) }
            assertFalse(handler.receive(payload()))
            host.afterRead = { host.local["A" to groupA] = PushConversation(groupA, false, true) }
            assertFalse(handler.receive(payload()))
            assertTrue(host.posted.isEmpty())
        }

    @Test fun failedUnregisterSameTokenAndProcessRestartCannotAdmitOldProfilePush() =
        runBlocking {
            val token = "same-device-token"
            val a = Host(PushOwner("A", 1, installationA))
            a.local["A" to groupA] = PushConversation(groupA, true, true)
            a.owner = null // Sign out persists first. Unregister fails.
            assertFalse(PushHandler(a).receive(payload()))
            assertFalse(PushHandler(a).receive(payload(1, installationA)))
            assertEquals(
                RegistrationAction.ENABLE,
                registrationAction(RegistrationInput(true, true, true, true, token), null, false),
            )
            val restartedB = Host(PushOwner("B", 1, installationB))
            restartedB.local["B" to groupB] = PushConversation(groupB, true, true)
            assertFalse(PushHandler(restartedB).receive(payload()))
            assertFalse(PushHandler(restartedB).receive(payload(1, installationA)))
            assertTrue(PushHandler(restartedB).receive(payload(id = groupB)))
            assertEquals(
                "B",
                restartedB.posted.values
                    .single()
                    .profile,
            )
        }

    @Test fun unconfiguredBuildAndTokenPermissionChangesChooseSafeRegistration() {
        val input = RegistrationInput(true, true, true, true, "new")
        assertEquals(RegistrationAction.NONE, registrationAction(input.copy(configured = false), "old", true))
        assertEquals(RegistrationAction.ENABLE, registrationAction(input, "old", true))
        assertEquals(RegistrationAction.NONE, registrationAction(input, "new", true))
        assertEquals(RegistrationAction.DISABLE, registrationAction(input.copy(permission = false), "new", true))
        assertEquals(RegistrationAction.DISABLE, registrationAction(input.copy(signedIn = false), "new", true))
        assertEquals(RegistrationAction.DISABLE, registrationAction(input.copy(appEnabled = false), "new", true))
        assertEquals(RegistrationAction.DISABLE, registrationAction(input.copy(token = null), "new", true))
    }
}

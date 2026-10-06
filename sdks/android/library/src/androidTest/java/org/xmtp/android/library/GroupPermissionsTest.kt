package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.xmtp_sdk.*

@RunWith(AndroidJUnit4::class)
class GroupPermissionsTest : BaseInstrumentedTest() {
    private lateinit var fixtures: TestFixtures
    private lateinit var alixClient: SDKClient
    private lateinit var boClient: SDKClient
    private lateinit var caroClient: SDKClient

    @Before override fun setUp() {
        super.setUp()
        fixtures = runBlocking { createFixtures() }
        alixClient = fixtures.alixClient
        boClient = fixtures.boClient
        caroClient = fixtures.caroClient
    }

    private suspend fun createGroup(mode: GroupPermissionMode = GroupPermissionMode.AdminOnly): Pair<Group, Group> {
        val group =
            boClient.conversations().createGroup(
                listOf(alixClient.inboxId(), caroClient.inboxId()),
                CreateGroupOptions(permissions = mode),
            )
        alixClient.conversations().sync()
        return group to alixClient.conversations().listGroups(null).single()
    }

    private suspend fun sync(
        boGroup: Group,
        alixGroup: Group,
    ) {
        boGroup.sync()
        alixGroup.sync()
    }

    private suspend fun denied(action: suspend () -> Unit) {
        val failure = runCatching { action() }.exceptionOrNull()
        assertTrue("expected PermissionDenied, got $failure", failure is XmtpException.PermissionDenied)
        val details = (failure as XmtpException.PermissionDenied).v1
        assertEquals("PermissionDenied", details.code)
        assertEquals(ErrorCategory.CONVERSATION, details.category)
        assertFalse(details.retryable)
    }

    @Test fun testGroupCanUpdateAdminList() =
        runBlocking {
            val (boGroup, alixGroup) = createGroup()
            assertFalse(boGroup.isAdmin(boClient.inboxId()))
            assertTrue(boGroup.isSuperAdmin(boClient.inboxId()))
            assertFalse(alixGroup.isCreator())
            assertFalse(alixGroup.isAdmin(alixClient.inboxId()))
            assertFalse(alixGroup.isSuperAdmin(alixClient.inboxId()))
            assertEquals(emptyList<InboxId>(), boGroup.listAdmins())
            assertEquals(listOf(boClient.inboxId()), boGroup.listSuperAdmins())

            assertEquals("", boGroup.state().name)
            denied { alixGroup.updateName("Alix group name") }
            sync(boGroup, alixGroup)
            assertEquals("", boGroup.state().name)
            boGroup.updateName("Bo group name")
            boGroup.addAdmin(alixClient.inboxId())
            sync(boGroup, alixGroup)
            assertTrue(alixGroup.isAdmin(alixClient.inboxId()))
            assertEquals(listOf(alixClient.inboxId()), boGroup.listAdmins())
            assertEquals(listOf(boClient.inboxId()), boGroup.listSuperAdmins())

            alixGroup.updateName("Alix group name")
            sync(boGroup, alixGroup)
            assertEquals("Alix group name", boGroup.state().name)
            assertEquals("Alix group name", alixGroup.state().name)
            boGroup.removeAdmin(alixClient.inboxId())
            sync(boGroup, alixGroup)
            assertFalse(alixGroup.isAdmin(alixClient.inboxId()))
            assertEquals(emptyList<InboxId>(), boGroup.listAdmins())
            assertEquals(listOf(boClient.inboxId()), boGroup.listSuperAdmins())
            denied { alixGroup.updateName("Alix group name 2") }
            sync(boGroup, alixGroup)
            assertEquals("Alix group name", boGroup.state().name)
        }
}

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

    @Test fun testGroupCreatedWithCorrectAdminList() =
        runBlocking {
            val (boGroup, alixGroup) = createGroup(GroupPermissionMode.AllMembers)
            assertFalse(boGroup.isAdmin(boClient.inboxId()))
            assertTrue(boGroup.isSuperAdmin(boClient.inboxId()))
            assertFalse(alixGroup.isCreator())
            assertFalse(alixGroup.isAdmin(alixClient.inboxId()))
            assertFalse(alixGroup.isSuperAdmin(alixClient.inboxId()))
            assertEquals(emptyList<InboxId>(), boGroup.listAdmins())
            assertEquals(listOf(boClient.inboxId()), boGroup.listSuperAdmins())
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

    @Test fun testGroupCanUpdateSuperAdminList() =
        runBlocking {
            val (boGroup, alixGroup) = createGroup()
            assertTrue(boGroup.isSuperAdmin(boClient.inboxId()))
            assertFalse(alixGroup.isSuperAdmin(alixClient.inboxId()))
            denied { alixGroup.removeSuperAdmin(boClient.inboxId()) }
            boGroup.addSuperAdmin(alixClient.inboxId())
            sync(boGroup, alixGroup)
            alixGroup.removeSuperAdmin(boClient.inboxId())
            sync(boGroup, alixGroup)
            assertEquals(listOf(alixClient.inboxId()), boGroup.listSuperAdmins())
            assertFalse(boGroup.isSuperAdmin(boClient.inboxId()))
        }

    @Test fun testGroupMembersAndPermissionLevel() =
        runBlocking {
            val (boGroup, alixGroup) = createGroup()

            suspend fun roles(
                admins: Int,
                superAdmins: Int,
                members: Int,
            ) {
                val values = boGroup.members()
                assertEquals(admins, values.count { it.permissionLevel == PermissionLevel.ADMIN })
                assertEquals(superAdmins, values.count { it.permissionLevel == PermissionLevel.SUPER_ADMIN })
                assertEquals(members, values.count { it.permissionLevel == PermissionLevel.MEMBER })
            }
            roles(0, 1, 2)
            boGroup.addAdmin(alixClient.inboxId())
            sync(boGroup, alixGroup)
            roles(1, 1, 1)
            boGroup.addSuperAdmin(caroClient.inboxId())
            sync(boGroup, alixGroup)
            roles(1, 2, 0)
        }

    @Test fun testCanCommitAfterInvalidPermissionsCommit() =
        runBlocking {
            val (boGroup, alixGroup) = createGroup(GroupPermissionMode.AllMembers)
            assertEquals("", boGroup.state().name)
            denied { alixGroup.addAdmin(alixClient.inboxId()) }
            sync(boGroup, alixGroup)
            assertEquals(emptyList<InboxId>(), boGroup.listAdmins())
            alixGroup.updateName("Alix group name")
            sync(boGroup, alixGroup)
            assertEquals("Alix group name", boGroup.state().name)
            assertEquals("Alix group name", alixGroup.state().name)
        }

    @Test fun testCanUpdatePermissions() =
        runBlocking {
            val (boGroup, alixGroup) = createGroup()
            denied { alixGroup.updateDescription("new group description") }
            sync(boGroup, alixGroup)
            assertEquals("", boGroup.state().description)
            assertEquals(
                PermissionPolicy.ADMIN,
                boGroup
                    .state()
                    .permissions.policySet.updateDescription,
            )
            boGroup.updatePermission(
                PermissionUpdateKind.UPDATE_METADATA,
                PermissionPolicy.ALLOW,
                MetadataFieldKind.DESCRIPTION,
            )
            sync(boGroup, alixGroup)
            assertEquals(
                PermissionPolicy.ALLOW,
                boGroup
                    .state()
                    .permissions.policySet.updateDescription,
            )
            alixGroup.updateDescription("Alix group description")
            sync(boGroup, alixGroup)
            assertEquals("Alix group description", boGroup.state().description)
            assertEquals("Alix group description", alixGroup.state().description)
        }

    private fun custom(appData: PermissionPolicy = PermissionPolicy.ALLOW) =
        PermissionPolicySet(
            addMember = PermissionPolicy.ADMIN,
            removeMember = PermissionPolicy.DENY,
            addAdmin = PermissionPolicy.ADMIN,
            removeAdmin = PermissionPolicy.SUPER_ADMIN,
            updateName = PermissionPolicy.ADMIN,
            updateDescription = PermissionPolicy.ALLOW,
            updateImage = PermissionPolicy.ADMIN,
            updateDisappearing = PermissionPolicy.ADMIN,
            updateAppData = appData,
        )

    @Test fun canCreateGroupWithCustomPermissions() =
        runBlocking {
            val policy = custom()
            val (boGroup, alixGroup) = createGroup(GroupPermissionMode.Custom(policy))
            assertEquals(policy, boGroup.state().permissions.policySet)
            assertEquals(policy, alixGroup.state().permissions.policySet)
        }

    @Test fun createGroupWithInvalidCustomPermissionsFails() =
        runBlocking {
            val invalid = custom(PermissionPolicy.ADMIN).copy(removeAdmin = PermissionPolicy.ALLOW)
            val failure = runCatching { createGroup(GroupPermissionMode.Custom(invalid)) }.exceptionOrNull()
            assertTrue("invalid admin policy was accepted", failure is XmtpException)
            alixClient.conversations().sync()
            assertTrue(alixClient.conversations().listGroups(null).isEmpty())
            val valid = custom(PermissionPolicy.ADMIN).copy(updateDisappearing = PermissionPolicy.ALLOW)
            val (_, alixGroup) = createGroup(GroupPermissionMode.Custom(valid))
            assertEquals(valid, alixGroup.state().permissions.policySet)
            assertEquals(1, alixClient.conversations().listGroups(null).size)
        }

    @Test fun canCreateGroupWithInboxIdCustomPermissions() =
        runBlocking {
            val policy = custom(PermissionPolicy.ADMIN)
            val boGroup =
                boClient.conversations().createGroup(
                    listOf(fixtures.alix, fixtures.caro),
                    CreateGroupOptions(permissions = GroupPermissionMode.Custom(policy)),
                )
            alixClient.conversations().sync()
            val alixGroup = alixClient.conversations().listGroups(null).single()
            assertEquals(policy, boGroup.state().permissions.policySet)
            assertEquals(policy, alixGroup.state().permissions.policySet)
            assertEquals(
                setOf(boClient.inboxId(), alixClient.inboxId(), caroClient.inboxId()),
                boGroup.members().map { it.inboxId }.toSet(),
            )
        }
}

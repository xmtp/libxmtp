package org.xmtp.android.library

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.library.libxmtp.PermissionLevel
import org.xmtp.android.library.messages.PrivateKey
import org.xmtp.android.library.messages.PrivateKeyBuilder
import org.xmtp.android.library.messages.walletAddress
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.GroupPermissionPreconfiguration
import uniffi.xmtpv3.org.xmtp.android.library.libxmtp.PermissionOption

@RunWith(AndroidJUnit4::class)
class GroupPermissionsTest {
    private lateinit var alixWallet: PrivateKeyBuilder
    private lateinit var boWallet: PrivateKeyBuilder
    private lateinit var alix: PrivateKey
    private lateinit var alixClient: Client
    private lateinit var bo: PrivateKey
    private lateinit var boClient: Client
    private lateinit var caroWallet: PrivateKeyBuilder
    private lateinit var caro: PrivateKey
    private lateinit var caroClient: Client
    private lateinit var fixtures: Fixtures

    @Before
    fun setUp() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        fixtures =
            fixtures(
                clientOptions = ClientOptions(
                    ClientOptions.Api(XMTPEnvironment.LOCAL, false),
                    enableV3 = true,
                    appContext = context
                )
            )
        alixWallet = fixtures.aliceAccount
        alix = fixtures.alice
        boWallet = fixtures.bobAccount
        bo = fixtures.bob
        caroWallet = fixtures.caroAccount
        caro = fixtures.caro

        alixClient = fixtures.aliceClient
        boClient = fixtures.bobClient
        caroClient = fixtures.caroClient
    }

    @Test
    fun testGroupCreatedWithCorrectAdminList() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress)) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        assert(!boGroup.isAdmin(boClient.inboxId))
        assert(boGroup.isSuperAdmin(boClient.inboxId))
        assert(!alixGroup.isCreator())
        assert(!alixGroup.isAdmin(alixClient.inboxId))
        assert(!alixGroup.isSuperAdmin(alixClient.inboxId))

        val adminList = runBlocking {
            boGroup.listAdmins()
        }
        val superAdminList = runBlocking {
            boGroup.listSuperAdmins()
        }
        assert(adminList.isEmpty())
        assert(!adminList.contains(boClient.inboxId))
        assert(superAdminList.size == 1)
        assert(superAdminList.contains(boClient.inboxId))
    }

    @Test
    fun testGroupCanUpdateAdminList() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress, caro.walletAddress), GroupPermissionPreconfiguration.ADMIN_ONLY) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        assert(!boGroup.isAdmin(boClient.inboxId))
        assert(boGroup.isSuperAdmin(boClient.inboxId))
        assert(!alixGroup.isCreator())
        assert(!alixGroup.isAdmin(alixClient.inboxId))
        assert(!alixGroup.isSuperAdmin(alixClient.inboxId))

        var adminList = runBlocking {
            boGroup.listAdmins()
        }
        var superAdminList = runBlocking {
            boGroup.listSuperAdmins()
        }
        assert(adminList.size == 0)
        assert(!adminList.contains(boClient.inboxId))
        assert(superAdminList.size == 1)
        assert(superAdminList.contains(boClient.inboxId))

        // Verify that alix can NOT  update group name
        assert(boGroup.name == "")
        val exception = assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.updateGroupName("Alix group name")
            }
        }
        assertEquals(exception.message, "Permission denied: Unable to update group name")
        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }
        assert(boGroup.name == "")
        assert(alixGroup.name == "")

        runBlocking {
            boGroup.addAdmin(alixClient.inboxId)
            boGroup.sync()
            alixGroup.sync()
        }

        adminList = runBlocking {
            boGroup.listAdmins()
        }
        superAdminList = runBlocking {
            boGroup.listSuperAdmins()
        }

        assert(alixGroup.isAdmin(alixClient.inboxId))
        assert(adminList.size == 1)
        assert(adminList.contains(alixClient.inboxId))
        assert(superAdminList.size == 1)

        // Verify that alix can now update group name
        runBlocking {
            boGroup.sync()
            alixGroup.sync()
            alixGroup.updateGroupName("Alix group name")
            alixGroup.sync()
            boGroup.sync()
        }
        assert(boGroup.name == "Alix group name")
        assert(alixGroup.name == "Alix group name")

        runBlocking {
            boGroup.removeAdmin(alixClient.inboxId)
            boGroup.sync()
            alixGroup.sync()
        }

        adminList = runBlocking {
            boGroup.listAdmins()
        }
        superAdminList = runBlocking {
            boGroup.listSuperAdmins()
        }

        assert(!alixGroup.isAdmin(alixClient.inboxId))
        assert(adminList.size == 0)
        assert(!adminList.contains(alixClient.inboxId))
        assert(superAdminList.size == 1)

        // Verify that alix can NOT  update group name
        val exception2 = assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.updateGroupName("Alix group name 2")
            }
        }
        assertEquals(exception.message, "Permission denied: Unable to update group name")
    }

    @Test
    fun testGroupCanUpdateSuperAdminList() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress, caro.walletAddress), GroupPermissionPreconfiguration.ADMIN_ONLY) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        assert(boGroup.isSuperAdmin(boClient.inboxId))
        assert(!alixGroup.isSuperAdmin(alixClient.inboxId))

        // Attempt to remove bo as a super admin by alix should fail since she is not a super admin
        val exception = assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.removeSuperAdmin(boClient.inboxId)
            }
        }
        assertEquals(exception.message, "Permission denied: Unable to remove super admin")

        // Make alix a super admin
        runBlocking {
            boGroup.addSuperAdmin(alixClient.inboxId)
            boGroup.sync()
            alixGroup.sync()
        }

        assert(alixGroup.isSuperAdmin(alixClient.inboxId))

        // Now alix should be able to remove bo as a super admin
        runBlocking {
            alixGroup.removeSuperAdmin(boClient.inboxId)
            alixGroup.sync()
            boGroup.sync()
        }

        val superAdminList = runBlocking {
            boGroup.listSuperAdmins()
        }

        assert(!superAdminList.contains(boClient.inboxId))
        assert(superAdminList.contains(alixClient.inboxId))
    }

    @Test
    fun testGroupMembersAndPermissionLevel() {
        val group = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress, caro.walletAddress), GroupPermissionPreconfiguration.ADMIN_ONLY) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        // Initial checks for group members and their permissions
        var members = runBlocking { group.members() }
        var admins = members.filter { it.permissionLevel == PermissionLevel.ADMIN }
        var superAdmins = members.filter { it.permissionLevel == PermissionLevel.SUPER_ADMIN }
        var regularMembers = members.filter { it.permissionLevel == PermissionLevel.MEMBER }

        assert(admins.size == 0)
        assert(superAdmins.size == 1)
        assert(regularMembers.size == 2)

        // Add alix as an admin
        runBlocking {
            group.addAdmin(alixClient.inboxId)
            group.sync()
            alixGroup.sync()
        }

        members = runBlocking { group.members() }
        admins = members.filter { it.permissionLevel == PermissionLevel.ADMIN }
        superAdmins = members.filter { it.permissionLevel == PermissionLevel.SUPER_ADMIN }
        regularMembers = members.filter { it.permissionLevel == PermissionLevel.MEMBER }

        assert(admins.size == 1)
        assert(superAdmins.size == 1)
        assert(regularMembers.size == 1)

        // Add caro as a super admin
        runBlocking {
            group.addSuperAdmin(caroClient.inboxId)
            group.sync()
            alixGroup.sync()
        }

        members = runBlocking { group.members() }
        admins = members.filter { it.permissionLevel == PermissionLevel.ADMIN }
        superAdmins = members.filter { it.permissionLevel == PermissionLevel.SUPER_ADMIN }
        regularMembers = members.filter { it.permissionLevel == PermissionLevel.MEMBER }

        assert(admins.size == 1)
        assert(superAdmins.size == 2)
        assert(regularMembers.isEmpty())
    }

    @Test
    fun testCanCommitAfterInvalidPermissionsCommit() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress, caro.walletAddress), GroupPermissionPreconfiguration.ALL_MEMBERS) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        // Verify that alix can NOT  add an admin
        assert(boGroup.name == "")
        val exception = assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.addAdmin(alixClient.inboxId)
            }
        }
        assertEquals(exception.message, "Permission denied: Unable to add admin")
        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }

        // Verify that alix can update group name
        runBlocking {
            boGroup.sync()
            alixGroup.sync()
            alixGroup.updateGroupName("Alix group name")
            alixGroup.sync()
            boGroup.sync()
        }
        assert(boGroup.name == "Alix group name")
        assert(alixGroup.name == "Alix group name")
    }

    @Test
    fun testCanUpdatePermissions() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress, caro.walletAddress), GroupPermissionPreconfiguration.ADMIN_ONLY) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        // Verify that alix can NOT update group name
        assert(boGroup.name == "")
        val exception = assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.updateGroupDescription("new group description")
            }
        }
        assertEquals(exception.message, "Permission denied: Unable to update group description")
        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }
        assertEquals(boGroup.permissionPolicySet().updateGroupDescriptionPolicy, PermissionOption.Admin)

        // Update group name permissions so Alix can update
        runBlocking {
            boGroup.updateGroupDescriptionPermission(PermissionOption.Allow)
            boGroup.sync()
            alixGroup.sync()
        }
        assertEquals(boGroup.permissionPolicySet().updateGroupDescriptionPolicy, PermissionOption.Allow)

        // Verify that alix can now update group name
        runBlocking {
            alixGroup.updateGroupDescription("Alix group description")
            alixGroup.sync()
            boGroup.sync()
        }
        assert(boGroup.description == "Alix group description")
        assert(alixGroup.description == "Alix group description")
    }

    @Test
    fun testCanUpdatePinnedFrameUrl() {
        val boGroup = runBlocking { boClient.conversations.newGroup(listOf(alix.walletAddress, caro.walletAddress), GroupPermissionPreconfiguration.ADMIN_ONLY) }
        runBlocking { alixClient.conversations.syncGroups() }
        val alixGroup = runBlocking { alixClient.conversations.listGroups().first() }

        // Verify that alix can NOT update pinned frame
        assert(boGroup.pinnedFrameUrl == "")
        val exception = assertThrows(XMTPException::class.java) {
            runBlocking {
                alixGroup.updateGroupPinnedFrameUrl("new pinned frame url")
            }
        }
        assertEquals(exception.message, "Permission denied: Unable to update pinned frame")
        runBlocking {
            alixGroup.sync()
            boGroup.sync()
        }
        assertEquals(boGroup.permissionPolicySet().updateGroupPinnedFrameUrlPolicy, PermissionOption.Admin)

        // Update group name permissions so Alix can update
        runBlocking {
            boGroup.updateGroupPinnedFrameUrlPermission(PermissionOption.Allow)
            boGroup.sync()
            alixGroup.sync()
        }
        assertEquals(boGroup.permissionPolicySet().updateGroupPinnedFrameUrlPolicy, PermissionOption.Allow)

        // Verify that alix can now update group name
        runBlocking {
            alixGroup.updateGroupPinnedFrameUrl("new pinned frame url 2")
            alixGroup.sync()
            boGroup.sync()
        }
        assert(boGroup.pinnedFrameUrl == "new pinned frame url 2")
        assert(alixGroup.pinnedFrameUrl == "new pinned frame url 2")
    }
}

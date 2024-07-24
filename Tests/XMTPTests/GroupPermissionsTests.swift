//
//  GroupPermissionTests.swift
//
//
//  Created by Cameron Voell on 5/29/24.
//

import CryptoKit
import XCTest
import XMTPiOS
import LibXMTP
import XMTPTestHelpers

@available(iOS 16, *)
class GroupPermissionTests: XCTestCase {
    // Use these fixtures to talk to the local node
    struct LocalFixtures {
        public var alice: PrivateKey!
        public var bob: PrivateKey!
        public var caro: PrivateKey!
        public var aliceClient: Client!
        public var bobClient: Client!
        public var caroClient: Client!
    }
    
    enum CryptoError: Error {
        case randomBytes, combinedPayload, hmacSignatureError
    }
    
    public func secureRandomBytes(count: Int) throws -> Data {
        var bytes = [UInt8](repeating: 0, count: count)

        // Fill bytes with secure random data
        let status = SecRandomCopyBytes(
            kSecRandomDefault,
            count,
            &bytes
        )

        // A status of errSecSuccess indicates success
        if status == errSecSuccess {
            return Data(bytes)
        } else {
            throw CryptoError.randomBytes
        }
    }
    
    func localFixtures() async throws -> LocalFixtures {
        let key = try secureRandomBytes(count: 32)
        let alice = try PrivateKey.generate()
        let aliceClient = try await Client.create(
            account: alice,
            options: .init(
                api: .init(env: .local, isSecure: false),
                codecs: [GroupUpdatedCodec()],
                enableV3: true,
                encryptionKey: key
            )
        )
        let bob = try PrivateKey.generate()
        let bobClient = try await Client.create(
            account: bob,
            options: .init(
                api: .init(env: .local, isSecure: false),
                codecs: [GroupUpdatedCodec()],
                enableV3: true,
                encryptionKey: key
            )
        )
        let caro = try PrivateKey.generate()
        let caroClient = try await Client.create(
            account: caro,
            options: .init(
                api: .init(env: .local, isSecure: false),
                codecs: [GroupUpdatedCodec()],
                enableV3: true,
                encryptionKey: key
            )
        )
        
        return .init(
            alice: alice,
            bob: bob,
            caro: caro,
            aliceClient: aliceClient,
            bobClient: bobClient,
            caroClient: caroClient
        )
    }
    
    func testGroupCreatedWithCorrectAdminList() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address])
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
        
        XCTAssertFalse(try bobGroup.isAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertTrue(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertFalse(try aliceGroup.isCreator())
        XCTAssertFalse(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertFalse(try aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
        
        let adminList = try bobGroup.listAdmins()
        let superAdminList = try bobGroup.listSuperAdmins()
        
        XCTAssertEqual(adminList.count, 0)
        XCTAssertFalse(adminList.contains(fixtures.bobClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        XCTAssertTrue(superAdminList.contains(fixtures.bobClient.inboxID))
    }
    
    func testGroupCanUpdateAdminList() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address, fixtures.caro.address], permissions: .adminOnly)
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
        
        XCTAssertFalse(try bobGroup.isAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertTrue(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertFalse(try aliceGroup.isCreator())
        XCTAssertFalse(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertFalse(try aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
        
        var adminList = try bobGroup.listAdmins()
        var superAdminList = try bobGroup.listSuperAdmins()
        XCTAssertEqual(adminList.count, 0)
        XCTAssertFalse(adminList.contains(fixtures.bobClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        XCTAssertTrue(superAdminList.contains(fixtures.bobClient.inboxID))
        
        // Verify that alice can NOT update group name
        XCTAssertEqual(try bobGroup.groupName(), "")
        await assertThrowsAsyncError(
            try await aliceGroup.updateGroupName(groupName: "Alice group name")
        )
        
        try await aliceGroup.sync()
        try await bobGroup.sync()
        XCTAssertEqual(try bobGroup.groupName(), "")
        XCTAssertEqual(try aliceGroup.groupName(), "")
        
        try await bobGroup.addAdmin(inboxId: fixtures.aliceClient.inboxID)
        try await bobGroup.sync()
        try await aliceGroup.sync()
        
        adminList = try bobGroup.listAdmins()
        superAdminList = try bobGroup.listSuperAdmins()
        
        XCTAssertTrue(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertEqual(adminList.count, 1)
        XCTAssertTrue(adminList.contains(fixtures.aliceClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        
        // Verify that alice can now update group name
        try await aliceGroup.updateGroupName(groupName: "Alice group name")
        try await aliceGroup.sync()
        try await bobGroup.sync()
        XCTAssertEqual(try bobGroup.groupName(), "Alice group name")
        XCTAssertEqual(try aliceGroup.groupName(), "Alice group name")
        
        try await bobGroup.removeAdmin(inboxId: fixtures.aliceClient.inboxID)
        try await bobGroup.sync()
        try await aliceGroup.sync()
        
        adminList = try bobGroup.listAdmins()
        superAdminList = try bobGroup.listSuperAdmins()
        
        XCTAssertFalse(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertEqual(adminList.count, 0)
        XCTAssertFalse(adminList.contains(fixtures.aliceClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        
        // Verify that alice can NOT update group name
        await assertThrowsAsyncError(
            try await aliceGroup.updateGroupName(groupName: "Alice group name 2")
        )
    }
    
    func testGroupCanUpdateSuperAdminList() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address, fixtures.caro.address], permissions: .adminOnly)
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!

        XCTAssertTrue(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertFalse(try aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))

        // Attempt to remove bob as a super admin by alice should fail since she is not a super admin
        await assertThrowsAsyncError(
            try await aliceGroup.removeSuperAdmin(inboxId: fixtures.bobClient.inboxID)
        )

        // Make alice a super admin
        try await bobGroup.addSuperAdmin(inboxId: fixtures.aliceClient.inboxID)
        try await bobGroup.sync()
        try await aliceGroup.sync()
        XCTAssertTrue(try aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))

        // Now alice should be able to remove bob as a super admin
        try await aliceGroup.removeSuperAdmin(inboxId: fixtures.bobClient.inboxID)
        try await aliceGroup.sync()
        try await bobGroup.sync()

        let superAdminList = try bobGroup.listSuperAdmins()
        XCTAssertFalse(superAdminList.contains(fixtures.bobClient.inboxID))
        XCTAssertTrue(superAdminList.contains(fixtures.aliceClient.inboxID))
    }
    
    func testGroupMembersAndPermissionLevel() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address, fixtures.caro.address], permissions: .adminOnly)
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!

        // Initial checks for group members and their permissions
        var members = try bobGroup.members
        var admins = members.filter { $0.permissionLevel == PermissionLevel.Admin }
        var superAdmins = members.filter { $0.permissionLevel == PermissionLevel.SuperAdmin }
        var regularMembers = members.filter { $0.permissionLevel == PermissionLevel.Member }

        XCTAssertEqual(admins.count, 0)
        XCTAssertEqual(superAdmins.count, 1)
        XCTAssertEqual(regularMembers.count, 2)

        // Add alice as an admin
        try await bobGroup.addAdmin(inboxId: fixtures.aliceClient.inboxID)
        try await bobGroup.sync()
        try await aliceGroup.sync()

        members = try bobGroup.members
        admins = members.filter { $0.permissionLevel == PermissionLevel.Admin }
        superAdmins = members.filter { $0.permissionLevel == PermissionLevel.SuperAdmin }
        regularMembers = members.filter { $0.permissionLevel == PermissionLevel.Member }

        XCTAssertEqual(admins.count, 1)
        XCTAssertEqual(superAdmins.count, 1)
        XCTAssertEqual(regularMembers.count, 1)

        // Add caro as a super admin
        try await bobGroup.addSuperAdmin(inboxId: fixtures.caroClient.inboxID)
        try await bobGroup.sync()
        try await aliceGroup.sync()

        members = try bobGroup.members
        admins = members.filter { $0.permissionLevel == PermissionLevel.Admin }
        superAdmins = members.filter { $0.permissionLevel == PermissionLevel.SuperAdmin }
        regularMembers = members.filter { $0.permissionLevel == PermissionLevel.Member }

        XCTAssertEqual(admins.count, 1)
        XCTAssertEqual(superAdmins.count, 2)
        XCTAssertTrue(regularMembers.isEmpty)
    }
    
    func testCanCommitAfterInvalidPermissionsCommit() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.address, fixtures.caro.address], permissions: .allMembers)
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
        
        // Verify that alice can NOT add an admin
        XCTAssertEqual(try bobGroup.groupName(), "")
        await assertThrowsAsyncError(
            try await aliceGroup.addAdmin(inboxId: fixtures.aliceClient.inboxID)
        )
        
        try await aliceGroup.sync()
        try await bobGroup.sync()
        
        // Verify that alice can update group name
        try await bobGroup.sync()
        try await aliceGroup.sync()
        try await aliceGroup.updateGroupName(groupName: "Alice group name")
        try await aliceGroup.sync()
        try await bobGroup.sync()
        
        XCTAssertEqual(try bobGroup.groupName(), "Alice group name")
        XCTAssertEqual(try aliceGroup.groupName(), "Alice group name")
    }
    
    func testCanUpdatePermissions() async throws {
            let fixtures = try await localFixtures()
            let bobGroup = try await fixtures.bobClient.conversations.newGroup(
                with: [fixtures.alice.address, fixtures.caro.address],
                permissions: .adminOnly
            )
            try await fixtures.aliceClient.conversations.sync()
            let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!

            // Verify that Alice cannot update group description
            XCTAssertEqual(try bobGroup.groupDescription(), "")
            await assertThrowsAsyncError(
                try await aliceGroup.updateGroupDescription(groupDescription: "new group description")
            )
            
            try await aliceGroup.sync()
            try await bobGroup.sync()
            XCTAssertEqual(try bobGroup.permissionPolicySet().updateGroupDescriptionPolicy, .admin)

            // Update group description permissions so Alice can update
            try await bobGroup.updateGroupDescriptionPermission(newPermissionOption: .allow)
            try await bobGroup.sync()
            try await aliceGroup.sync()
            XCTAssertEqual(try bobGroup.permissionPolicySet().updateGroupDescriptionPolicy, .allow)

            // Verify that Alice can now update group description
            try await aliceGroup.updateGroupDescription(groupDescription: "Alice group description")
            try await aliceGroup.sync()
            try await bobGroup.sync()
            XCTAssertEqual(try bobGroup.groupDescription(), "Alice group description")
            XCTAssertEqual(try aliceGroup.groupDescription(), "Alice group description")
        }

        func testCanUpdatePinnedFrameUrl() async throws {
            let fixtures = try await localFixtures()
            let bobGroup = try await fixtures.bobClient.conversations.newGroup(
                with: [fixtures.alice.address, fixtures.caro.address],
                permissions: .adminOnly,
                pinnedFrameUrl: "initial url"
            )
            try await fixtures.aliceClient.conversations.sync()
            let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!

            // Verify that Alice cannot update group pinned frame url
            XCTAssertEqual(try bobGroup.groupPinnedFrameUrl(), "initial url")
            await assertThrowsAsyncError(
                try await aliceGroup.updateGroupPinnedFrameUrl(groupPinnedFrameUrl: "https://foo/bar.com")
            )
            
            try await aliceGroup.sync()
            try await bobGroup.sync()
            XCTAssertEqual(try bobGroup.permissionPolicySet().updateGroupPinnedFrameUrlPolicy, .admin)

            // Update group pinned frame url permissions so Alice can update
            try await bobGroup.updateGroupPinnedFrameUrlPermission(newPermissionOption: .allow)
            try await bobGroup.sync()
            try await aliceGroup.sync()
            XCTAssertEqual(try bobGroup.permissionPolicySet().updateGroupPinnedFrameUrlPolicy, .allow)

            // Verify that Alice can now update group pinned frame url
            try await aliceGroup.updateGroupPinnedFrameUrl(groupPinnedFrameUrl: "https://foo/barz.com")
            try await aliceGroup.sync()
            try await bobGroup.sync()
            XCTAssertEqual(try bobGroup.groupPinnedFrameUrl(), "https://foo/barz.com")
            XCTAssertEqual(try aliceGroup.groupPinnedFrameUrl(), "https://foo/barz.com")
        }
    
    func testCanCreateGroupWithCustomPermissions() async throws {
        let fixtures = try await localFixtures()
        let permissionPolicySet = PermissionPolicySet(
            addMemberPolicy: PermissionOption.admin,
            removeMemberPolicy: PermissionOption.deny,
            addAdminPolicy: PermissionOption.admin,
            removeAdminPolicy: PermissionOption.superAdmin,
            updateGroupNamePolicy: PermissionOption.admin,
            updateGroupDescriptionPolicy: PermissionOption.allow,
            updateGroupImagePolicy: PermissionOption.admin,
            updateGroupPinnedFrameUrlPolicy: PermissionOption.deny
        )
        let _bobGroup = try await fixtures.bobClient.conversations.newGroupCustomPermissions(
            with: [fixtures.alice.address, fixtures.caro.address],
            permissionPolicySet: permissionPolicySet,
            pinnedFrameUrl: "initial url"
        )
        
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
        
        let alicePermissionSet = try aliceGroup.permissionPolicySet()
        XCTAssert(alicePermissionSet.addMemberPolicy == PermissionOption.admin)
        XCTAssert(alicePermissionSet.removeMemberPolicy == PermissionOption.deny)
        XCTAssert(alicePermissionSet.addAdminPolicy == PermissionOption.admin)
        XCTAssert(alicePermissionSet.removeAdminPolicy == PermissionOption.superAdmin)
        XCTAssert(alicePermissionSet.updateGroupNamePolicy == PermissionOption.admin)
        XCTAssert(alicePermissionSet.updateGroupDescriptionPolicy == PermissionOption.allow)
        XCTAssert(alicePermissionSet.updateGroupImagePolicy == PermissionOption.admin)
        XCTAssert(alicePermissionSet.updateGroupPinnedFrameUrlPolicy == PermissionOption.deny)
    }
    
    func testCreateGroupWithInvalidPermissionsFails() async throws {
        let fixtures = try await localFixtures()
        // Add / remove admin can not be set to "allow"
        let permissionPolicySetInvalid = PermissionPolicySet(
            addMemberPolicy: PermissionOption.admin,
            removeMemberPolicy: PermissionOption.deny,
            addAdminPolicy: PermissionOption.allow,
            removeAdminPolicy: PermissionOption.superAdmin,
            updateGroupNamePolicy: PermissionOption.admin,
            updateGroupDescriptionPolicy: PermissionOption.allow,
            updateGroupImagePolicy: PermissionOption.admin,
            updateGroupPinnedFrameUrlPolicy: PermissionOption.deny
        )
        await assertThrowsAsyncError(
            try await fixtures.bobClient.conversations.newGroupCustomPermissions(
                with: [fixtures.alice.address, fixtures.caro.address],
                permissionPolicySet: permissionPolicySetInvalid,
                pinnedFrameUrl: "initial url"
            )
        )
    }
}

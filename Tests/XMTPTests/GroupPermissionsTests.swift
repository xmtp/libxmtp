//
//  GroupPermissionTests.swift
//
//
//  Created by Cameron Voell on 5/29/24.
//

import CryptoKit
import XCTest
@testable import XMTPiOS
import LibXMTP
import XMTPTestHelpers

@available(iOS 16, *)
class GroupPermissionTests: XCTestCase {
    // Use these fixtures to talk to the local node
    struct LocalFixtures {
        var alice: PrivateKey!
        var bob: PrivateKey!
        var caro: PrivateKey!
        var aliceClient: Client!
        var bobClient: Client!
        var caroClient: Client!
    }
    
    func localFixtures() async throws -> LocalFixtures {
        let key = try Crypto.secureRandomBytes(count: 32)
        let alice = try PrivateKey.generate()
        let aliceClient = try await Client.create(
            account: alice,
            options: .init(
                api: .init(env: .local, isSecure: false),
                codecs: [GroupUpdatedCodec()],
                mlsAlpha: true,
                mlsEncryptionKey: key
            )
        )
        let bob = try PrivateKey.generate()
        let bobClient = try await Client.create(
            account: bob,
            options: .init(
                api: .init(env: .local, isSecure: false),
                codecs: [GroupUpdatedCodec()],
                mlsAlpha: true,
                mlsEncryptionKey: key
            )
        )
        let caro = try PrivateKey.generate()
        let caroClient = try await Client.create(
            account: caro,
            options: .init(
                api: .init(env: .local, isSecure: false),
                codecs: [GroupUpdatedCodec()],
                mlsAlpha: true,
                mlsEncryptionKey: key
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
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.walletAddress])
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
        
        XCTAssertTrue(try bobGroup.isAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertTrue(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertFalse(try aliceGroup.isCreator())
        XCTAssertFalse(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertFalse(try aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
        
        let adminList = try bobGroup.listAdmins()
        let superAdminList = try bobGroup.listSuperAdmins()
        
        XCTAssertEqual(adminList.count, 1)
        XCTAssertTrue(adminList.contains(fixtures.bobClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        XCTAssertTrue(superAdminList.contains(fixtures.bobClient.inboxID))
    }
    
    func testGroupCanUpdateAdminList() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.walletAddress, fixtures.caro.walletAddress], permissions: .adminOnly)
        try await fixtures.aliceClient.conversations.sync()
        let aliceGroup = try await fixtures.aliceClient.conversations.groups().first!
        
        XCTAssertTrue(try bobGroup.isAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertTrue(try bobGroup.isSuperAdmin(inboxId: fixtures.bobClient.inboxID))
        XCTAssertFalse(try aliceGroup.isCreator())
        XCTAssertFalse(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertFalse(try aliceGroup.isSuperAdmin(inboxId: fixtures.aliceClient.inboxID))
        
        var adminList = try bobGroup.listAdmins()
        var superAdminList = try bobGroup.listSuperAdmins()
        XCTAssertEqual(adminList.count, 1)
        XCTAssertTrue(adminList.contains(fixtures.bobClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        XCTAssertTrue(superAdminList.contains(fixtures.bobClient.inboxID))
        
        // Verify that alice can NOT update group name
        XCTAssertEqual(try bobGroup.groupName(), "New Group")
        await assertThrowsAsyncError(
            try await aliceGroup.updateGroupName(groupName: "Alice group name")
        )
        
        try await aliceGroup.sync()
        try await bobGroup.sync()
        XCTAssertEqual(try bobGroup.groupName(), "New Group")
        XCTAssertEqual(try aliceGroup.groupName(), "New Group")
        
        try await bobGroup.addAdmin(inboxId: fixtures.aliceClient.inboxID)
        try await bobGroup.sync()
        try await aliceGroup.sync()
        
        adminList = try bobGroup.listAdmins()
        superAdminList = try bobGroup.listSuperAdmins()
        
        XCTAssertTrue(try aliceGroup.isAdmin(inboxId: fixtures.aliceClient.inboxID))
        XCTAssertEqual(adminList.count, 2)
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
        XCTAssertEqual(adminList.count, 1)
        XCTAssertFalse(adminList.contains(fixtures.aliceClient.inboxID))
        XCTAssertEqual(superAdminList.count, 1)
        
        // Verify that alice can NOT update group name
        await assertThrowsAsyncError(
            try await aliceGroup.updateGroupName(groupName: "Alice group name 2")
        )
    }
    
    func testGroupCanUpdateSuperAdminList() async throws {
        let fixtures = try await localFixtures()
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.walletAddress, fixtures.caro.walletAddress], permissions: .adminOnly)
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
        let bobGroup = try await fixtures.bobClient.conversations.newGroup(with: [fixtures.alice.walletAddress, fixtures.caro.walletAddress], permissions: .adminOnly)
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

}

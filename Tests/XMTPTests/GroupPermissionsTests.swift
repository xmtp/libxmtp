import LibXMTP
import XCTest
import XMTPTestHelpers
import XMTPiOS

@available(iOS 16, *)
class GroupPermissionTests: XCTestCase {
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

	func testGroupCreatedWithCorrectAdminList() async throws {
		let fixtures = try await fixtures()
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID])
		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		XCTAssertFalse(
			try boGroup.isAdmin(inboxId: fixtures.boClient.inboxID))
		XCTAssertTrue(
			try boGroup.isSuperAdmin(inboxId: fixtures.boClient.inboxID))
		let isAlixGroupCreator = try await alixGroup.isCreator()
		XCTAssertFalse(isAlixGroupCreator)
		XCTAssertFalse(
			try alixGroup.isAdmin(inboxId: fixtures.alixClient.inboxID))
		XCTAssertFalse(
			try alixGroup.isSuperAdmin(inboxId: fixtures.alixClient.inboxID))

		let adminList = try boGroup.listAdmins()
		let superAdminList = try boGroup.listSuperAdmins()

		XCTAssertEqual(adminList.count, 0)
		XCTAssertFalse(adminList.contains(fixtures.boClient.inboxID))
		XCTAssertEqual(superAdminList.count, 1)
		XCTAssertTrue(superAdminList.contains(fixtures.boClient.inboxID))
		try fixtures.cleanUpDatabases()
	}

	func testGroupCanUpdateAdminList() async throws {
		let fixtures = try await fixtures()
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID, fixtures.caroClient.inboxID],
			permissions: .adminOnly)
		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		XCTAssertFalse(
			try boGroup.isAdmin(inboxId: fixtures.boClient.inboxID))
		XCTAssertTrue(
			try boGroup.isSuperAdmin(inboxId: fixtures.boClient.inboxID))
		let isAlixGroupCreator = try await alixGroup.isCreator()
		XCTAssertFalse(isAlixGroupCreator)
		XCTAssertFalse(
			try alixGroup.isAdmin(inboxId: fixtures.alixClient.inboxID))
		XCTAssertFalse(
			try alixGroup.isSuperAdmin(inboxId: fixtures.alixClient.inboxID))

		var adminList = try boGroup.listAdmins()
		var superAdminList = try boGroup.listSuperAdmins()
		XCTAssertEqual(adminList.count, 0)
		XCTAssertFalse(adminList.contains(fixtures.boClient.inboxID))
		XCTAssertEqual(superAdminList.count, 1)
		XCTAssertTrue(superAdminList.contains(fixtures.boClient.inboxID))

		// Verify that alix can NOT update group name
		XCTAssertEqual(try boGroup.name(), "")
		await assertThrowsAsyncError(
			try await alixGroup.updateName(name: "alix group name")
		)

		try await alixGroup.sync()
		try await boGroup.sync()
		XCTAssertEqual(try boGroup.name(), "")
		XCTAssertEqual(try alixGroup.name(), "")

		try await boGroup.addAdmin(inboxId: fixtures.alixClient.inboxID)
		try await boGroup.sync()
		try await alixGroup.sync()

		adminList = try boGroup.listAdmins()
		superAdminList = try boGroup.listSuperAdmins()

		XCTAssertTrue(
			try alixGroup.isAdmin(inboxId: fixtures.alixClient.inboxID))
		XCTAssertEqual(adminList.count, 1)
		XCTAssertTrue(adminList.contains(fixtures.alixClient.inboxID))
		XCTAssertEqual(superAdminList.count, 1)

		// Verify that alix can now update group name
		try await alixGroup.updateName(name: "alix group name")
		try await alixGroup.sync()
		try await boGroup.sync()
		XCTAssertEqual(try boGroup.name(), "alix group name")
		XCTAssertEqual(try alixGroup.name(), "alix group name")

		try await boGroup.removeAdmin(inboxId: fixtures.alixClient.inboxID)
		try await boGroup.sync()
		try await alixGroup.sync()

		adminList = try boGroup.listAdmins()
		superAdminList = try boGroup.listSuperAdmins()

		XCTAssertFalse(
			try alixGroup.isAdmin(inboxId: fixtures.alixClient.inboxID))
		XCTAssertEqual(adminList.count, 0)
		XCTAssertFalse(adminList.contains(fixtures.alixClient.inboxID))
		XCTAssertEqual(superAdminList.count, 1)

		// Verify that alix can NOT update group name
		await assertThrowsAsyncError(
			try await alixGroup.updateName(
				name: "alix group name 2")
		)
		try fixtures.cleanUpDatabases()
	}

	func testGroupCanUpdateSuperAdminList() async throws {
		let fixtures = try await fixtures()
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID, fixtures.caroClient.inboxID],
			permissions: .adminOnly)
		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		XCTAssertTrue(
			try boGroup.isSuperAdmin(inboxId: fixtures.boClient.inboxID))
		XCTAssertFalse(
			try alixGroup.isSuperAdmin(inboxId: fixtures.alixClient.inboxID))

		// Attempt to remove bo as a super admin by alix should fail since she is not a super admin
		await assertThrowsAsyncError(
			try await alixGroup.removeSuperAdmin(
				inboxId: fixtures.boClient.inboxID)
		)

		// Make alix a super admin
		try await boGroup.addSuperAdmin(inboxId: fixtures.alixClient.inboxID)
		try await boGroup.sync()
		try await alixGroup.sync()
		XCTAssertTrue(
			try alixGroup.isSuperAdmin(inboxId: fixtures.alixClient.inboxID))

		// Now alix should be able to remove bo as a super admin
		try await alixGroup.removeSuperAdmin(
			inboxId: fixtures.boClient.inboxID)
		try await alixGroup.sync()
		try await boGroup.sync()

		let superAdminList = try boGroup.listSuperAdmins()
		XCTAssertFalse(superAdminList.contains(fixtures.boClient.inboxID))
		XCTAssertTrue(superAdminList.contains(fixtures.alixClient.inboxID))
		try fixtures.cleanUpDatabases()
	}

	func testGroupMembersAndPermissionLevel() async throws {
		let fixtures = try await fixtures()
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID, fixtures.caroClient.inboxID],
			permissions: .adminOnly)
		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		// Initial checks for group members and their permissions
		var members = try await boGroup.members
		var admins = members.filter {
			$0.permissionLevel == PermissionLevel.Admin
		}
		var superAdmins = members.filter {
			$0.permissionLevel == PermissionLevel.SuperAdmin
		}
		var regularMembers = members.filter {
			$0.permissionLevel == PermissionLevel.Member
		}

		XCTAssertEqual(admins.count, 0)
		XCTAssertEqual(superAdmins.count, 1)
		XCTAssertEqual(regularMembers.count, 2)

		// Add alix as an admin
		try await boGroup.addAdmin(inboxId: fixtures.alixClient.inboxID)
		try await boGroup.sync()
		try await alixGroup.sync()

		members = try await boGroup.members
		admins = members.filter { $0.permissionLevel == PermissionLevel.Admin }
		superAdmins = members.filter {
			$0.permissionLevel == PermissionLevel.SuperAdmin
		}
		regularMembers = members.filter {
			$0.permissionLevel == PermissionLevel.Member
		}

		XCTAssertEqual(admins.count, 1)
		XCTAssertEqual(superAdmins.count, 1)
		XCTAssertEqual(regularMembers.count, 1)

		// Add caro as a super admin
		try await boGroup.addSuperAdmin(inboxId: fixtures.caroClient.inboxID)
		try await boGroup.sync()
		try await alixGroup.sync()

		members = try await boGroup.members
		admins = members.filter { $0.permissionLevel == PermissionLevel.Admin }
		superAdmins = members.filter {
			$0.permissionLevel == PermissionLevel.SuperAdmin
		}
		regularMembers = members.filter {
			$0.permissionLevel == PermissionLevel.Member
		}

		XCTAssertEqual(admins.count, 1)
		XCTAssertEqual(superAdmins.count, 2)
		XCTAssertTrue(regularMembers.isEmpty)
		try fixtures.cleanUpDatabases()
	}

	func testCanCommitAfterInvalidPermissionsCommit() async throws {
		let fixtures = try await fixtures()
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID, fixtures.caroClient.inboxID],
			permissions: .allMembers)
		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		// Verify that alix can NOT add an admin
		XCTAssertEqual(try boGroup.name(), "")
		await assertThrowsAsyncError(
			try await alixGroup.addAdmin(inboxId: fixtures.alixClient.inboxID)
		)

		try await alixGroup.sync()
		try await boGroup.sync()

		// Verify that alix can update group name
		try await boGroup.sync()
		try await alixGroup.sync()
		try await alixGroup.updateName(name: "alix group name")
		try await alixGroup.sync()
		try await boGroup.sync()

		XCTAssertEqual(try boGroup.name(), "alix group name")
		XCTAssertEqual(try alixGroup.name(), "alix group name")
		try fixtures.cleanUpDatabases()
	}

	func testCanUpdatePermissions() async throws {
		let fixtures = try await fixtures()
		let boGroup = try await fixtures.boClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID, fixtures.caroClient.inboxID],
			permissions: .adminOnly
		)
		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		// Verify that alix cannot update group description
		XCTAssertEqual(try boGroup.description(), "")
		await assertThrowsAsyncError(
			try await alixGroup.updateDescription(
				description: "new group description")
		)

		try await alixGroup.sync()
		try await boGroup.sync()
		XCTAssertEqual(
			try boGroup.permissionPolicySet().updateGroupDescriptionPolicy,
			.admin)

		// Update group description permissions so alix can update
		try await boGroup.updateDescriptionPermission(
			newPermissionOption: .allow)
		try await boGroup.sync()
		try await alixGroup.sync()
		XCTAssertEqual(
			try boGroup.permissionPolicySet().updateGroupDescriptionPolicy,
			.allow)

		// Verify that alix can now update group description
		try await alixGroup.updateDescription(
			description: "alix group description")
		try await alixGroup.sync()
		try await boGroup.sync()
		XCTAssertEqual(
			try boGroup.description(), "alix group description")
		XCTAssertEqual(
			try alixGroup.description(), "alix group description")
		try fixtures.cleanUpDatabases()
	}

	func testCanCreateGroupWithCustomPermissions() async throws {
		let fixtures = try await fixtures()
		let permissionPolicySet = PermissionPolicySet(
			addMemberPolicy: PermissionOption.admin,
			removeMemberPolicy: PermissionOption.deny,
			addAdminPolicy: PermissionOption.admin,
			removeAdminPolicy: PermissionOption.superAdmin,
			updateGroupNamePolicy: PermissionOption.admin,
			updateGroupDescriptionPolicy: PermissionOption.allow,
			updateGroupImagePolicy: PermissionOption.admin,
			updateMessageDisappearingPolicy: PermissionOption.allow
		)
		_ = try await fixtures.boClient.conversations
			.newGroupCustomPermissions(
				with: [
					fixtures.alixClient.inboxID, fixtures.caroClient.inboxID,
				],
				permissionPolicySet: permissionPolicySet
			)

		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		let alixPermissionSet = try alixGroup.permissionPolicySet()
		XCTAssert(alixPermissionSet.addMemberPolicy == PermissionOption.admin)
		XCTAssert(
			alixPermissionSet.removeMemberPolicy == PermissionOption.deny)
		XCTAssert(alixPermissionSet.addAdminPolicy == PermissionOption.admin)
		XCTAssert(
			alixPermissionSet.removeAdminPolicy == PermissionOption.superAdmin)
		XCTAssert(
			alixPermissionSet.updateGroupNamePolicy == PermissionOption.admin)
		XCTAssert(
			alixPermissionSet.updateGroupDescriptionPolicy
				== PermissionOption.allow)
		XCTAssert(
			alixPermissionSet.updateGroupImagePolicy == PermissionOption.admin)
		try fixtures.cleanUpDatabases()
	}

	func testCanCreateGroupWithInboxIdCustomPermissions() async throws {
		let fixtures = try await fixtures()
		let permissionPolicySet = PermissionPolicySet(
			addMemberPolicy: PermissionOption.admin,
			removeMemberPolicy: PermissionOption.deny,
			addAdminPolicy: PermissionOption.admin,
			removeAdminPolicy: PermissionOption.superAdmin,
			updateGroupNamePolicy: PermissionOption.admin,
			updateGroupDescriptionPolicy: PermissionOption.allow,
			updateGroupImagePolicy: PermissionOption.admin,
			updateMessageDisappearingPolicy: PermissionOption.allow
		)
		_ = try await fixtures.boClient.conversations
			.newGroupCustomPermissionsWithIdentities(
				with: [fixtures.alix.identity, fixtures.caro.identity],
				permissionPolicySet: permissionPolicySet
			)

		try await fixtures.alixClient.conversations.sync()
		let alixGroup = try await fixtures.alixClient.conversations
			.listGroups().first!

		let alixPermissionSet = try alixGroup.permissionPolicySet()
		XCTAssert(alixPermissionSet.addMemberPolicy == PermissionOption.admin)
		XCTAssert(
			alixPermissionSet.removeMemberPolicy == PermissionOption.deny)
		XCTAssert(alixPermissionSet.addAdminPolicy == PermissionOption.admin)
		XCTAssert(
			alixPermissionSet.removeAdminPolicy == PermissionOption.superAdmin)
		XCTAssert(
			alixPermissionSet.updateGroupNamePolicy == PermissionOption.admin)
		XCTAssert(
			alixPermissionSet.updateGroupDescriptionPolicy
				== PermissionOption.allow)
		XCTAssert(
			alixPermissionSet.updateGroupImagePolicy == PermissionOption.admin)
		try fixtures.cleanUpDatabases()
	}

	func testCreateGroupWithInvalidPermissionsFails() async throws {
		let fixtures = try await fixtures()
		// Add / remove admin can not be set to "allow"
		let permissionPolicySetInvalid = PermissionPolicySet(
			addMemberPolicy: PermissionOption.admin,
			removeMemberPolicy: PermissionOption.deny,
			addAdminPolicy: PermissionOption.allow,
			removeAdminPolicy: PermissionOption.superAdmin,
			updateGroupNamePolicy: PermissionOption.admin,
			updateGroupDescriptionPolicy: PermissionOption.allow,
			updateGroupImagePolicy: PermissionOption.admin,
			updateMessageDisappearingPolicy: PermissionOption.allow
		)
		await assertThrowsAsyncError(
			try await fixtures.boClient.conversations
				.newGroupCustomPermissions(
					with: [
						fixtures.alixClient.inboxID,
						fixtures.caroClient.inboxID,
					],
					permissionPolicySet: permissionPolicySetInvalid
				)
		)
		try fixtures.cleanUpDatabases()
	}
}

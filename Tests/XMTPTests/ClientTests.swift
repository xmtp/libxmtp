import Foundation
import LibXMTP
import XCTest
import XMTPTestHelpers

@testable import XMTPiOS

@available(iOS 15, *)
class ClientTests: XCTestCase {
	func testTakesAWallet() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let clientOptions: ClientOptions = ClientOptions(
			api: ClientOptions.Api(
				env: XMTPEnvironment.local, isSecure: false),
			dbEncryptionKey: key
		)
		let fakeWallet = try PrivateKey.generate()
		_ = try await Client.create(account: fakeWallet, options: clientOptions)
	}

	func testPassingEncryptionKey() async throws {
		let bo = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		_ = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)
	}

	func testStaticCanMessage() async throws {
		let fixtures = try await fixtures()
		let notOnNetwork = try PrivateKey.generate()

		let canMessageList = try await Client.canMessage(
			accountIdentities: [
				fixtures.alix.identity,
				notOnNetwork.identity,
				fixtures.bo.identity,
			],
			api: ClientOptions.Api(env: .local, isSecure: false)
		)

		let expectedResults: [String: Bool] = [
			fixtures.alix.walletAddress.lowercased(): true,
			notOnNetwork.walletAddress.lowercased(): false,
			fixtures.bo.walletAddress.lowercased(): true,
		]

		for (address, expected) in expectedResults {
			XCTAssertEqual(
				canMessageList[address.lowercased()], expected,
				"Failed for address: \(address)")
		}
	}

	func testStaticInboxState() async throws {
		let fixtures = try await fixtures()

		let inboxStates = try await Client.inboxStatesForInboxIds(
			inboxIds: [
				fixtures.alixClient.inboxID,
				fixtures.boClient.inboxID,
			],
			api: ClientOptions.Api(env: .local, isSecure: false)
		)

		XCTAssertEqual(
			inboxStates.first!.recoveryIdentity.identifier,
			fixtures.alix.walletAddress.lowercased()
		)
		XCTAssertEqual(
			inboxStates.last!.recoveryIdentity.identifier,
			fixtures.bo.walletAddress.lowercased()
		)
	}

	func testCanDeleteDatabase() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		var boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		_ = try await boClient.conversations.newGroup(with: [alixClient.inboxID]
		)
		try await boClient.conversations.sync()

		var groupCount = try await boClient.conversations.listGroups().count
		XCTAssertEqual(groupCount, 1)

		assert(!boClient.dbPath.isEmpty)
		try boClient.deleteLocalDatabase()

		boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		try await boClient.conversations.sync()
		groupCount = try await boClient.conversations.listGroups().count
		XCTAssertEqual(groupCount, 0)
	}

	func testCanDropReconnectDatabase() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		let boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		_ = try await boClient.conversations.newGroup(with: [alixClient.inboxID]
		)
		try await boClient.conversations.sync()

		var groupCount = try await boClient.conversations.listGroups().count
		XCTAssertEqual(groupCount, 1)

		try boClient.dropLocalDatabaseConnection()

		await assertThrowsAsyncError(
			try await boClient.conversations.listGroups())

		try await boClient.reconnectLocalDatabase()

		groupCount = try await boClient.conversations.listGroups().count
		XCTAssertEqual(groupCount, 1)
	}

	func testCanMessage() async throws {
		let fixtures = try await fixtures()
		let notOnNetwork = try PrivateKey.generate()

		let canMessage = try await fixtures.alixClient.canMessage(
			identity: fixtures.bo.identity)
		let cannotMessage = try await fixtures.alixClient.canMessage(
			identity: notOnNetwork.identity)
		XCTAssertTrue(canMessage)
		XCTAssertFalse(cannotMessage)
	}

	func testPreAuthenticateToInboxCallback() async throws {
		let fakeWallet = try PrivateKey.generate()
		let expectation = XCTestExpectation(
			description: "preAuthenticateToInboxCallback is called")
		let key = try Crypto.secureRandomBytes(count: 32)

		let preAuthenticateToInboxCallback: () async throws -> Void = {
			print("preAuthenticateToInboxCallback called")
			expectation.fulfill()
		}

		let opts = ClientOptions(
			api: ClientOptions.Api(env: .local, isSecure: false),
			preAuthenticateToInboxCallback: preAuthenticateToInboxCallback,
			dbEncryptionKey: key
		)
		do {
			_ = try await Client.create(account: fakeWallet, options: opts)
			await XCTWaiter().fulfillment(of: [expectation], timeout: 30)
		} catch {
			XCTFail("Error: \(error)")
		}
	}

	func testPassingEncryptionKeyAndDatabaseDirectory() async throws {
		let bo = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		let client = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		let bundleClient = try await Client.build(
			publicIdentity: bo.identity,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		XCTAssertEqual(client.inboxID, bundleClient.inboxID)
		XCTAssertEqual(client.dbPath, bundleClient.dbPath)
		XCTAssert(!client.installationID.isEmpty)

		await assertThrowsAsyncError(
			_ = try await Client.build(
				publicIdentity: bo.identity,
				options: .init(
					api: .init(env: .local, isSecure: false),
					dbEncryptionKey: key,
					dbDirectory: nil
				)
			)
		)
	}

	func testEncryptionKeyCanDecryptCorrectly() async throws {
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		let key = try Crypto.secureRandomBytes(count: 32)

		let boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db"
			)
		)

		_ = try await boClient.conversations.newGroup(with: [
			alixClient.inboxID
		])

		let key2 = try Crypto.secureRandomBytes(count: 32)
		await assertThrowsAsyncError(
			try await Client.create(
				account: bo,
				options: .init(
					api: .init(env: .local, isSecure: false),
					dbEncryptionKey: key2,
					dbDirectory: "xmtp_db"
				)
			)
		)
	}

	func testCanGetAnInboxIdFromAddress() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let bo = try PrivateKey.generate()
		let alix = try PrivateKey.generate()
		let boClient = try await Client.create(
			account: bo,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		let alixClient = try await Client.create(
			account: alix,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)
		let boInboxId = try await alixClient.inboxIdFromIdentity(
			identity: bo.identity)
		XCTAssertEqual(boClient.inboxID, boInboxId)
	}

	func testCreatesAClient() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			dbEncryptionKey: key
		)

		let inboxId = try await Client.getOrCreateInboxId(
			api: options.api, publicIdentity: alix.identity)
		let alixClient = try await Client.create(
			account: alix,
			options: options
		)

		XCTAssertEqual(inboxId, alixClient.inboxID)

		let alixClient2 = try await Client.build(
			publicIdentity: alix.identity,
			options: options
		)

		XCTAssertEqual(
			alixClient2.publicIdentity.identifier,
			alixClient.publicIdentity.identifier)
		XCTAssertEqual(alixClient2.inboxID, alixClient.inboxID)
	}

	func testRevokeInstallations() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()

		let alixClient = try await Client.create(
			account: alix,
			options: ClientOptions.init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		let alixClient2 = try await Client.create(
			account: alix,
			options: ClientOptions.init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db1"
			)
		)

		let alixClient3 = try await Client.create(
			account: alix,
			options: ClientOptions.init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db2"
			)
		)

		let state = try await alixClient3.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 3)

		try await alixClient3.revokeInstallations(
			signingKey: alix, installationIds: [alixClient2.installationID])

		let newState = try await alixClient3.inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(newState.installations.count, 2)
	}

	func testRevokesAllOtherInstallations() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()

		let alixClient = try await Client.create(
			account: alix,
			options: ClientOptions.init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		let alixClient2 = try await Client.create(
			account: alix,
			options: ClientOptions.init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db1"
			)
		)

		let alixClient3 = try await Client.create(
			account: alix,
			options: ClientOptions.init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db2"
			)
		)

		let state = try await alixClient3.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 3)
		XCTAssert(state.installations.first?.createdAt != nil)

		try await alixClient3.revokeAllOtherInstallations(signingKey: alix)

		let newState = try await alixClient3.inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(newState.installations.count, 1)
	}

	func testsCanFindOthersInboxStates() async throws {
		let fixtures = try await fixtures()
		let states = try await fixtures.alixClient.inboxStatesForInboxIds(
			refreshFromNetwork: true,
			inboxIds: [fixtures.boClient.inboxID, fixtures.caroClient.inboxID]
		)
		XCTAssertEqual(
			states.first!.recoveryIdentity.identifier,
			fixtures.bo.walletAddress.lowercased())
		XCTAssertEqual(
			states.last!.recoveryIdentity.identifier,
			fixtures.caro.walletAddress.lowercased())
	}

	func testAddAccounts() async throws {
		let fixtures = try await fixtures()
		let alix2Wallet = try PrivateKey.generate()
		let alix3Wallet = try PrivateKey.generate()

		try await fixtures.alixClient.addAccount(newAccount: alix2Wallet)
		try await fixtures.alixClient.addAccount(newAccount: alix3Wallet)

		let state = try await fixtures.alixClient.inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 1)
		XCTAssertEqual(state.identities.count, 3)
		XCTAssertEqual(
			state.recoveryIdentity.identifier,
			fixtures.alix.walletAddress.lowercased())
		XCTAssertEqual(
			state.identities.map { $0.identifier }.sorted(),
			[
				alix2Wallet.walletAddress.lowercased(),
				alix3Wallet.walletAddress.lowercased(),
				fixtures.alix.walletAddress.lowercased(),
			].sorted()
		)
	}

	func testAddAccountsWithExistingInboxIds() async throws {
		let fixtures = try await fixtures()

		await assertThrowsAsyncError(
			try await fixtures.alixClient.addAccount(newAccount: fixtures.bo))

		XCTAssert(fixtures.boClient.inboxID != fixtures.alixClient.inboxID)
		try await fixtures.alixClient.addAccount(
			newAccount: fixtures.bo, allowReassignInboxId: true)

		let state = try await fixtures.alixClient.inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(state.identities.count, 2)

		let inboxId = try await fixtures.alixClient.inboxIdFromIdentity(
			identity: fixtures.bo.identity)
		XCTAssertEqual(inboxId, fixtures.alixClient.inboxID)
	}

	func testRemovingAccounts() async throws {
		let fixtures = try await fixtures()
		let alix2Wallet = try PrivateKey.generate()
		let alix3Wallet = try PrivateKey.generate()

		try await fixtures.alixClient.addAccount(newAccount: alix2Wallet)
		try await fixtures.alixClient.addAccount(newAccount: alix3Wallet)

		var state = try await fixtures.alixClient.inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(state.identities.count, 3)
		XCTAssertEqual(
			state.recoveryIdentity.identifier,
			fixtures.alix.walletAddress.lowercased())

		try await fixtures.alixClient.removeAccount(
			recoveryAccount: fixtures.alix,
			identityToRemove: alix2Wallet.identity
		)

		state = try await fixtures.alixClient.inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(state.identities.count, 2)
		XCTAssertEqual(
			state.recoveryIdentity.identifier,
			fixtures.alix.walletAddress.lowercased())
		XCTAssertEqual(
			state.identities.map { $0.identifier }.sorted(),
			[
				alix3Wallet.walletAddress.lowercased(),
				fixtures.alix.walletAddress.lowercased(),
			].sorted()
		)
		XCTAssertEqual(state.installations.count, 1)

		// Cannot remove the recovery address
		await assertThrowsAsyncError(
			try await fixtures.alixClient.removeAccount(
				recoveryAccount: alix3Wallet,
				identityToRemove: fixtures.alix.identity
			))
	}

	func testSignatures() async throws {
		let fixtures = try await fixtures()

		// Signing with installation key
		let signature = try fixtures.alixClient.signWithInstallationKey(
			message: "Testing")
		XCTAssertTrue(
			try fixtures.alixClient.verifySignature(
				message: "Testing", signature: signature))
		XCTAssertFalse(
			try fixtures.alixClient.verifySignature(
				message: "Not Testing", signature: signature))

		let alixInstallationId = fixtures.alixClient.installationID

		XCTAssertTrue(
			try fixtures.alixClient.verifySignatureWithInstallationId(
				message: "Testing",
				signature: signature,
				installationId: alixInstallationId
			))
		XCTAssertFalse(
			try fixtures.alixClient.verifySignatureWithInstallationId(
				message: "Not Testing",
				signature: signature,
				installationId: alixInstallationId
			))
		XCTAssertFalse(
			try fixtures.alixClient.verifySignatureWithInstallationId(
				message: "Testing",
				signature: signature,
				installationId: fixtures.boClient.installationID
			))
		XCTAssertTrue(
			try fixtures.boClient.verifySignatureWithInstallationId(
				message: "Testing",
				signature: signature,
				installationId: alixInstallationId
			))

		try fixtures.alixClient.deleteLocalDatabase()
		let key = try Crypto.secureRandomBytes(count: 32)
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			dbEncryptionKey: key
		)

		// Creating a new client
		let alixClient2 = try await Client.create(
			account: fixtures.alix,
			options: options
		)

		XCTAssertTrue(
			try alixClient2.verifySignatureWithInstallationId(
				message: "Testing",
				signature: signature,
				installationId: alixInstallationId
			))
		XCTAssertFalse(
			try alixClient2.verifySignatureWithInstallationId(
				message: "Testing2",
				signature: signature,
				installationId: alixInstallationId
			))
	}

	func testCreatesAClientManually() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alix = try PrivateKey.generate()
		let options = ClientOptions.init(
			api: .init(env: .local, isSecure: false),
			dbEncryptionKey: key
		)

		let inboxId = try await Client.getOrCreateInboxId(
			api: options.api, publicIdentity: alix.identity)
		let client = try await Client.ffiCreateClient(
			identity: alix.identity, clientOptions: options)
		let sigRequest = client.ffiSignatureRequest()
		try await sigRequest!.addEcdsaSignature(
			signatureBytes: try alix.sign(sigRequest!.signatureText())
				.rawData)
		try await client.ffiRegisterIdentity(signatureRequest: sigRequest!)
		let state = try await client.inboxState(refreshFromNetwork: true)
			.identities
		let canMessage = try await client.canMessage(identities: state)[
			state.first!.identifier]

		XCTAssertTrue(canMessage == true)
		XCTAssertEqual(inboxId, client.inboxID)
	}

	func testCanManageAddRemoveManually() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alixWallet = try PrivateKey.generate()
		let boWallet = try PrivateKey.generate()

		let options = ClientOptions(
			api: .init(env: .local, isSecure: false),
			dbEncryptionKey: key
		)

		let alix = try await Client.create(
			account: alixWallet, options: options)

		var inboxState = try await alix.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.identities.count, 1)

		let sigRequest = try await alix.ffiAddIdentity(
			identityToAdd: boWallet.identity)
		let signedMessage = try await boWallet.sign(sigRequest.signatureText())
			.rawData

		try await sigRequest.addEcdsaSignature(signatureBytes: signedMessage)
		try await alix.ffiApplySignatureRequest(signatureRequest: sigRequest)

		inboxState = try await alix.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.identities.count, 2)

		let sigRequest2 = try await alix.ffiRevokeIdentity(
			identityToRemove: boWallet.identity)
		let signedMessage2 = try await alixWallet.sign(
			sigRequest2.signatureText()
		).rawData

		try await sigRequest2.addEcdsaSignature(signatureBytes: signedMessage2)
		try await alix.ffiApplySignatureRequest(signatureRequest: sigRequest2)

		inboxState = try await alix.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.identities.count, 1)
	}

	func testCanManageRevokeManually() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alixWallet = try PrivateKey.generate()

		let dbDirPath = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp_db").path
		let dbDirPath2 = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp_db2").path
		let dbDirPath3 = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp_db3").path

		try FileManager.default.createDirectory(
			atPath: dbDirPath, withIntermediateDirectories: true)
		try FileManager.default.createDirectory(
			atPath: dbDirPath2, withIntermediateDirectories: true)
		try FileManager.default.createDirectory(
			atPath: dbDirPath3, withIntermediateDirectories: true)

		let alix = try await Client.create(
			account: alixWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: dbDirPath
			)
		)

		let alix2 = try await Client.create(
			account: alixWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: dbDirPath2
			)
		)

		let alix3 = try await Client.create(
			account: alixWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: dbDirPath3
			)
		)

		var inboxState = try await alix3.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.installations.count, 3)

		let sigRequest = try await alix.ffiRevokeInstallations(ids: [
			alix2.installationID.hexToData
		])
		let signedMessage = try await alixWallet.sign(
			sigRequest.signatureText()
		).rawData

		try await sigRequest.addEcdsaSignature(signatureBytes: signedMessage)
		try await alix.ffiApplySignatureRequest(signatureRequest: sigRequest)

		inboxState = try await alix.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.installations.count, 2)

		let sigRequest2 = try await alix.ffiRevokeAllOtherInstallations()
		let signedMessage2 = try await alixWallet.sign(
			sigRequest2.signatureText()
		).rawData

		try await sigRequest2.addEcdsaSignature(signatureBytes: signedMessage2)
		try await alix.ffiApplySignatureRequest(signatureRequest: sigRequest2)

		inboxState = try await alix.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.installations.count, 1)
	}

	func testPersistentLogging() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let fakeWallet = try PrivateKey.generate()

		// Create a specific log directory for this test
		let fileManager = FileManager.default
		let logDirectory = fileManager.temporaryDirectory
			.appendingPathComponent("xmtp_test_logs")

		if fileManager.fileExists(atPath: logDirectory.path) {
			try fileManager.removeItem(at: logDirectory)
		}
		try fileManager.createDirectory(
			at: logDirectory, withIntermediateDirectories: true)

		// Clear any existing logs in this directory
		Client.clearXMTPLogs(customLogDirectory: logDirectory)

		// Make sure logging is deactivated at the end of the test
		defer {
			Client.deactivatePersistentLibXMTPLogWriter()
		}

		// Activate persistent logging with a small number of log files and DEBUG level
		Client.activatePersistentLibXMTPLogWriter(
			logLevel: .debug,
			rotationSchedule: .hourly,
			maxFiles: 3,
			customLogDirectory: logDirectory
		)

		// Create a client
		let client = try await Client.create(
			account: fakeWallet,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		// Create a group with only the client as a member
		let group = try await client.conversations.newGroup(with: [])
		try await client.conversations.sync()

		// Verify the group was created
		let groups = try await client.conversations.listGroups()
		XCTAssertEqual(groups.count, 1)

		// Deactivate logging to ensure files are flushed
		Client.deactivatePersistentLibXMTPLogWriter()

		// Verify logs were created
		let logFiles = Client.getXMTPLogFilePaths(
			customLogDirectory: logDirectory)
		XCTAssertFalse(logFiles.isEmpty, "No log files were created")

		// Print log files content to console and check for inbox ID
		print("Found \(logFiles.count) log files:")
		var foundInboxId = false

		for filePath in logFiles {
			print("\n--- Log file: \(filePath) ---")
			do {
				let content = try String(contentsOfFile: filePath)
				// Print first 1000 chars to avoid overwhelming the console
				let truncatedContent = content.prefix(1000)
				print(
					"\(truncatedContent)\(content.count > 1000 ? "...(truncated)" : "")"
				)

				// Check if the inbox ID appears in the logs
				if content.contains(client.inboxID) {
					foundInboxId = true
					print("Found inbox ID in logs: \(client.inboxID)")
				}
			} catch {
				print("Error reading log file: \(error.localizedDescription)")
			}
		}

		XCTAssertTrue(
			foundInboxId, "Inbox ID \(client.inboxID) not found in logs")

		// Test clearing logs
		Client.clearXMTPLogs(customLogDirectory: logDirectory)
		let logFilesAfterClear = Client.getXMTPLogFilePaths(
			customLogDirectory: logDirectory)
		XCTAssertEqual(
			logFilesAfterClear.count, 0, "Logs were not cleared properly")
	}

	func testNetworkDebugInformation() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alixWallet = try PrivateKey.generate()
		let alix = try await Client.create(
			account: alixWallet,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		alix.debugInformation.clearAllStatistics()
		// Start streaming messages
		let streamTask = Task {
			for try await _ in await alix.conversations.streamAllMessages() {
				// Just consume the stream
			}
		}

		// Create a group and send a message
		let group = try await alix.conversations.newGroup(with: [])
		_ = try await group.send(content: "hi")
		try await Task.sleep(nanoseconds: 2_000_000_000)  // 2 seconds

		let aggregateStats2 = alix.debugInformation.aggregateStatistics
		print("Aggregate Stats Create:\n\(aggregateStats2)")

		let apiStats2 = alix.debugInformation.apiStatistics
		XCTAssertEqual(0, apiStats2.uploadKeyPackage)
		XCTAssertEqual(0, apiStats2.fetchKeyPackage)
		XCTAssertEqual(6, apiStats2.sendGroupMessages)
		XCTAssertEqual(0, apiStats2.sendWelcomeMessages)
		XCTAssertEqual(1, apiStats2.queryWelcomeMessages)
		XCTAssertEqual(1, apiStats2.subscribeWelcomes)

		let identityStats2 = alix.debugInformation.identityStatistics
		XCTAssertEqual(0, identityStats2.publishIdentityUpdate)
		XCTAssertEqual(2, identityStats2.getIdentityUpdatesV2)
		XCTAssertEqual(0, identityStats2.getInboxIds)
		XCTAssertEqual(0, identityStats2.verifySmartContractWalletSignature)

		// Cancel the streaming task
		streamTask.cancel()
	}

	func testUploadArchiveDebugInformation() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alixWallet = try PrivateKey.generate()
		let alix = try await Client.create(
			account: alixWallet,
			options: .init(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key
			)
		)

		let uploadKey = try await alix.debugInformation.uploadDebugInformation()
		XCTAssertFalse(uploadKey.isEmpty)
	}

	func testCanSeeKeyPackageStatus() async throws {
		let fixtures = try await fixtures()
		let api = ClientOptions.Api(env: .local, isSecure: true)

		try await Client.connectToApiBackend(api: api)

		guard
			let inboxState = try await Client.inboxStatesForInboxIds(
				inboxIds: [fixtures.alixClient.inboxID],
				api: api
			).first
		else {
			XCTFail("No inbox state found")
			return
		}

		let installationIds = inboxState.installations.map { $0.id }

		let keyPackageStatus =
			try await Client.keyPackageStatusesForInstallationIds(
				installationIds: installationIds,
				api: api
			)

		for installationId in keyPackageStatus.keys {
			guard let thisKPStatus = keyPackageStatus[installationId] else {
				XCTFail(
					"Missing key package status for installationId: \(installationId)"
				)
				continue
			}

			let notBeforeDate: String
			if let notBefore = thisKPStatus.lifetime?.notBefore {
				notBeforeDate =
					Date(timeIntervalSince1970: TimeInterval(notBefore))
					.description
			} else {
				notBeforeDate = "null"
			}

			let notAfterDate: String
			if let notAfter = thisKPStatus.lifetime?.notAfter {
				notAfterDate =
					Date(timeIntervalSince1970: TimeInterval(notAfter))
					.description
			} else {
				notAfterDate = "null"
			}
			print(
				"inst: \(installationId) - valid from: \(notBeforeDate) to: \(notAfterDate)"
			)
			print("error code: \(thisKPStatus.validationError ?? "none")")

			if let notBefore = thisKPStatus.lifetime?.notBefore,
				let notAfter = thisKPStatus.lifetime?.notAfter
			{
				let expectedDuration: UInt64 = UInt64(3600 * 24 * 28 * 3 + 3600)
				XCTAssertEqual(notAfter - notBefore, expectedDuration)
			}
		}
	}

	func testCanBeBuiltOffline() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let wallet = try PrivateKey.generate()
		let options = ClientOptions(
			api: .init(env: .local, isSecure: false),
			dbEncryptionKey: key
		)

		let client = try await Client.create(account: wallet, options: options)

		client.debugInformation.clearAllStatistics()
		print("Initial stats: \(client.debugInformation.aggregateStatistics)")

		let builtClient = try await Client.build(
			publicIdentity: client.publicIdentity,
			options: options,
			inboxId: client.inboxID
		)

		print(
			"Post-build stats: \(builtClient.debugInformation.aggregateStatistics)"
		)
		XCTAssertEqual(client.inboxID, builtClient.inboxID)

		let fixtures = try await fixtures()  // Assuming this provides alixClient and boClient

		let group = try await builtClient.conversations.newGroup(
			with: [fixtures.alixClient.inboxID])
		try await group.send(content: "howdy")

		let alixDm = try await fixtures.alixClient.conversations
			.newConversation(with: builtClient.inboxID)
		try await alixDm.send(content: "howdy")

		let boGroup = try await fixtures.boClient.conversations
			.newGroupWithIdentities(
				with: [builtClient.publicIdentity])
		try await boGroup.send(content: "howdy")

		try await builtClient.conversations.syncAllConversations()
		let convos = try await builtClient.conversations.list()

		XCTAssertEqual(convos.count, 3)
	}

	func testCannotCreateMoreThan5Installations() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let wallet = try PrivateKey.generate()

		var clients: [Client] = []

		for i in 0..<5 {
			let client = try await Client.create(
				account: wallet,
				options: ClientOptions(
					api: .init(env: .local, isSecure: false),
					dbEncryptionKey: key,
					dbDirectory: "xmtp_db_\(i)"
				)
			)
			clients.append(client)
		}

		let state = try await clients[0].inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 5)

		// Attempt to create a 6th installation, should throw
		await assertThrowsAsyncError(
			_ = try await Client.create(
				account: wallet,
				options: ClientOptions(
					api: .init(env: .local, isSecure: false),
					dbEncryptionKey: key,
					dbDirectory: "xmtp_db_5"
				)
			)
		)

		let boWallet = try PrivateKey.generate()
		let boClient = try await Client.create(
			account: boWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: try Crypto.secureRandomBytes(count: 32),
				dbDirectory: "xmtp_bo"
			)
		)

		let group = try await boClient.conversations.newGroup(with: [
			clients[2].inboxID
		])
		let members = try await group.members
		let alixMember = members.first { $0.inboxId == clients[0].inboxID }
		XCTAssertNotNil(alixMember)

		let inboxState = try await boClient.inboxStatesForInboxIds(
			refreshFromNetwork: true,
			inboxIds: [alixMember!.inboxId]
		)
		XCTAssertEqual(inboxState.first?.installations.count, 5)

		try await clients[0].revokeInstallations(
			signingKey: wallet,
			installationIds: [clients[4].installationID]
		)

		let stateAfterRevoke = try await clients[0].inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(stateAfterRevoke.installations.count, 4)

		let sixthClient = try await Client.create(
			account: wallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: "xmtp_db_6"
			)
		)

		let finalState = try await clients[0].inboxState(
			refreshFromNetwork: true)
		XCTAssertEqual(finalState.installations.count, 5)
	}

	func testStaticRevokeOneOfFiveInstallations() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let wallet = try PrivateKey.generate()

		var clients: [Client] = []

		for i in 0..<5 {
			let client = try await Client.create(
				account: wallet,
				options: ClientOptions(
					api: .init(env: .local, isSecure: false),
					dbEncryptionKey: key,
					dbDirectory: "xmtp_db_\(i)"
				)
			)
			clients.append(client)
		}

		var state = try await clients.last!.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 5)

		let toRevokeId = clients[1].installationID

		try await Client.revokeInstallations(
			api: .init(env: .local, isSecure: false),
			signingKey: wallet,
			inboxId: clients.first!.inboxID,
			installationIds: [toRevokeId]
		)

		state = try await clients.last!.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(state.installations.count, 4)

		let remainingIds = state.installations.map { $0.id }
		XCTAssertFalse(remainingIds.contains(toRevokeId))
	}

	func testStaticRevokeAllInstalltions() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let wallet = try PrivateKey.generate()

		var clients: [Client] = []

		for i in 0..<5 {
			let client = try await Client.create(
				account: wallet,
				options: ClientOptions(
					api: .init(env: .local, isSecure: false),
					dbEncryptionKey: key,
					dbDirectory: "xmtp_db_\(i)"
				)
			)
			clients.append(client)
		}

		var states = try await Client.inboxStatesForInboxIds(
			inboxIds: [
				clients.last!.inboxID
			],
			api: ClientOptions.Api(env: .local, isSecure: false)
		)
		XCTAssertEqual(states.first!.installations.count, 5)

		let toRevokeIds = states.first!.installations.map { $0.id }

		try await Client.revokeInstallations(
			api: .init(env: .local, isSecure: false),
			signingKey: wallet,
			inboxId: clients.first!.inboxID,
			installationIds: toRevokeIds
		)

		states = try await Client.inboxStatesForInboxIds(
			inboxIds: [
				clients.last!.inboxID
			],
			api: ClientOptions.Api(env: .local, isSecure: false)
		)
		XCTAssertEqual(states.first!.installations.count, 0)
	}

	func testStaticRevokeInstallationsManually() async throws {
		let key = try Crypto.secureRandomBytes(count: 32)
		let alixWallet = try PrivateKey.generate()

		let dbDirPath = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp_db").path
		let dbDirPath2 = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp_db2").path
		let dbDirPath3 = FileManager.default.temporaryDirectory
			.appendingPathComponent("xmtp_db3").path

		try FileManager.default.createDirectory(
			atPath: dbDirPath, withIntermediateDirectories: true)
		try FileManager.default.createDirectory(
			atPath: dbDirPath2, withIntermediateDirectories: true)
		try FileManager.default.createDirectory(
			atPath: dbDirPath3, withIntermediateDirectories: true)

		let alix = try await Client.create(
			account: alixWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: dbDirPath
			)
		)

		let alix2 = try await Client.create(
			account: alixWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: dbDirPath2
			)
		)

		let alix3 = try await Client.create(
			account: alixWallet,
			options: ClientOptions(
				api: .init(env: .local, isSecure: false),
				dbEncryptionKey: key,
				dbDirectory: dbDirPath3
			)
		)

		var inboxState = try await alix3.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.installations.count, 3)

		let sigRequest = try await Client.ffiRevokeInstallations(
			api: .init(env: .local, isSecure: false),
			publicIdentity: alixWallet.identity,
			inboxId: alix.inboxID,
			installationIds: [
				alix2.installationID
			])
		let signedMessage = try await alixWallet.sign(
			sigRequest.signatureText()
		).rawData

		try await sigRequest.addEcdsaSignature(signatureBytes: signedMessage)
		try await Client.ffiApplySignatureRequest(
			api: .init(env: .local, isSecure: false),
			signatureRequest: sigRequest)

		inboxState = try await alix.inboxState(refreshFromNetwork: true)
		XCTAssertEqual(inboxState.installations.count, 2)
	}
}

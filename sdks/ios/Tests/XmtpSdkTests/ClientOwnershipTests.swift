import Foundation
import XCTest
import XmtpSdk

/// Host client code in `runtime/SDKClient.swift` and `runtime/SDKTypes.swift`:
/// the weak message-to-client registry, default storage resolution, and the
/// static backend helpers.
final class ClientOwnershipTests: XCTestCase {
	/// A message finds the client that read it. After `end()`, or after the app
	/// releases the client, message actions fail with `ClientClosed`.
	func testMessageClientFollowsItsOwner() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: root) }
		let options = liveOptions(storage: StorageOptions(location: .directory(directory: root.path)))
		try await withClients { scope in
			let host = try await scope.create(signer: generateLocalSigner(), options: options)
			let group = try await host.conversations().createGroup(members: [InboxId]())
			let sentId = try await group.sendText(text: "owned")
			let rows = try await group.messages(options: nil)
			let sent = try XCTUnwrap(rows.first { $0.id == sentId })
			XCTAssertTrue(try sent.client() === host)
			let identity = host.identity()
			let inboxId = host.inboxId()
			try await host.end()
			try await assertClientClosed { _ = try sent.client() }
			try await assertClientClosed { _ = try await sent.refresh() }

			var orphan: Message?
			weak var released: SDKClient?
			// The scope must not hold this client: the test checks that the app
			// releases it without `end()`.
			do {
				let shortLived = try await SDKClient.build(identity: identity, options: options, inboxId: inboxId)
				released = shortLived
				let shortGroup = try await shortLived.conversations().createGroup(members: [InboxId]())
				let orphanId = try await shortGroup.sendText(text: "weak owner")
				orphan = try await shortGroup.messages(options: nil).first { $0.id == orphanId }
			}
			XCTAssertNil(released, "The registry kept the released client alive")
			let message = try XCTUnwrap(orphan)
			try await assertClientClosed { _ = try message.client() }
			try await assertClientClosed { _ = try await message.refresh() }
		}
	}

	/// Copies of one message compare equal and hash alike. A change to any
	/// stored field makes them unequal.
	func testMessageEqualityComparesItsData() async throws {
		try await withClients { scope in
			let client = try await scope.create(signer: generateLocalSigner())
			let group = try await client.conversations().createGroup(members: [InboxId]())
			let id = try await group.sendText(text: "compared")
			let lookup = try await client.conversations().getMessageById(id: id)
			let message = try XCTUnwrap(lookup)
			let copy = Message(data: message.data)
			XCTAssertEqual(copy, message)
			XCTAssertEqual(copy.hashValue, message.hashValue)
			var changedStatus = message.data
			changedStatus.deliveryStatus = message.deliveryStatus == .failed ? .published : .failed
			XCTAssertNotEqual(Message(data: changedStatus), message)
			var changedCursor = message.data
			changedCursor.deliveryCursor = nil
			XCTAssertNotEqual(Message(data: changedCursor), message)
		}
	}

	/// Default storage resolves to the app's Application Support folder. A
	/// build with an unknown identity there creates no database.
	func testDefaultStorageUsesApplicationSupport() async throws {
		let bundle = try XCTUnwrap(Bundle.main.bundleIdentifier, "The test host has no bundle identifier")
		let support = try XCTUnwrap(FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first)
		let appFolder = support.appendingPathComponent(bundle)
		let xmtp = appFolder.appendingPathComponent("xmtp")
		// Remove only a folder that this test created.
		if !FileManager.default.fileExists(atPath: appFolder.path) {
			addTeardownBlock { try? FileManager.default.removeItem(at: appFolder) }
		}
		try await withClients { scope in
			let options = liveOptions(storage: StorageOptions(location: .default))
			let client = try await scope.create(signer: generateLocalSigner(), options: options)
			let storedPath = try await client.storage().path()
			let path = try XCTUnwrap(storedPath)
			let inboxFolder = URL(fileURLWithPath: path).deletingLastPathComponent()
			defer { try? FileManager.default.removeItem(at: inboxFolder) }
			XCTAssertTrue(path.hasPrefix(xmtp.path + "/"), "\(path) is not under \(xmtp.path)")
			XCTAssertTrue(path.hasSuffix("/\(client.inboxId())/xmtp.db3"), path)
			XCTAssertTrue(FileManager.default.fileExists(atPath: path))
			try await client.end()

			let before = databases(under: xmtp)
			do {
				let unknown = try await generateLocalSigner().identity()
				_ = try await scope.build(identity: unknown, options: options)
				XCTFail("A build opened a database with no identity")
			} catch XmtpError.IdentityNotFound {}
			XCTAssertEqual(databases(under: xmtp), before, "A failed build created a database")
		}
	}

	/// The static helpers reach the backend without a client, through options
	/// or a connected backend.
	func testStaticBackendHelpers() async throws {
		try await withClients { scope in
			let signer = await generateLocalSigner()
			let client = try await scope.create(signer: signer)
			let identity = try await signer.identity()
			let backendOptions = BackendOptions(url: liveBackendURL)
			let connected = try await Backend.connect(options: backendOptions)
			for backend in [BackendSource.connected(backend: connected), .options(options: backendOptions)] {
				let inboxId = try await SDKClient.inboxIdFor(identity: identity, backend: backend)
				XCTAssertEqual(inboxId, client.inboxId())
				let reachable = try await SDKClient.canMessage(identities: [identity], backend: backend)
				XCTAssertEqual(reachable, ["ethereum:\(identity.identifier)": true])
				let configuration = try await SDKClient.fetchServerConfiguration(backend: backend)
				XCTAssertEqual(configuration.identifier, client.serverConfiguration().identifier)
			}
			do {
				_ = try await SDKClient.fetchServerConfiguration(
					backend: .options(options: BackendOptions(url: "http://127.0.0.1:1")),
				)
				XCTFail("An unreachable backend returned a configuration")
			} catch XmtpError.ConfigurationUnavailable {}
		}
	}

	private func assertClientClosed(_ action: () async throws -> Void) async throws {
		do {
			try await action()
			XCTFail("The action did not fail with ClientClosed")
		} catch XmtpError.ClientClosed {}
	}

	private func databases(under root: URL) -> Set<String> {
		let files = FileManager.default.enumerator(atPath: root.path)?.allObjects as? [String] ?? []
		return Set(files.filter { $0.hasSuffix(".db3") })
	}
}

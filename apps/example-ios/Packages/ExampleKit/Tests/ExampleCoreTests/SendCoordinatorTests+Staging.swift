import ExampleCore
import Foundation
import Testing

extension SendCoordinatorTests {
	@Test(arguments: [false, true]) func stagedSourceIsRemovedAfterSDKCreate(fails: Bool) async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
		let original = root.appendingPathComponent("original.png")
		let bytes = Data([1, 2, 3, 4])
		try bytes.write(to: original)
		let staging = AttachmentStagingFiles(temporaryDirectory: root.appendingPathComponent("temp"))
		let staged = try staging.newFile()
		try FileManager.default.copyItem(at: original, to: staged)
		let fixture = AttachmentFixture()
		var copied: Data?
		fixture.gateway.createHook = { url, _ in
			copied = try Data(contentsOf: url)
			if fails {
				throw ChatError.unavailable
			}
			return fixture.file
		}
		let origin = DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 1)
		let store = AttachmentStore(gateway: fixture.gateway, staging: staging, drafts: fixture.drafts,
		                            coordinator: fixture.coordinator, conversationID: "chat",
		                            origin: origin,
		                            admit: { _ in true }, allowed: { true })
		store.send(.load)
		await store.wait()
		store.send(.stage(staged, "original.png"))
		await store.wait()
		#expect(copied == bytes)
		#expect(!FileManager.default.fileExists(atPath: staged.path))
		#expect(try Data(contentsOf: original) == bytes)
		#expect(try fixture.drafts.all().count == (fails ? 0 : 1))
		if let draft = try fixture.drafts.all().first {
			store.send(.discard(draft.id)); await store.wait()
		}
		#expect(try Data(contentsOf: original) == bytes)
		try FileManager.default.removeItem(at: root)
	}

	@Test func abandonedStagingCleanupIsConfinedAndDoesNotFollowLinks() throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
		let original = root.appendingPathComponent("original.png")
		let bytes = Data([4, 3, 2, 1])
		try bytes.write(to: original)
		let staging = AttachmentStagingFiles(temporaryDirectory: root.appendingPathComponent("temp"))
		let abandoned = try staging.newFile()
		try bytes.write(to: abandoned)
		let unrelated = staging.directory.appendingPathComponent("not-a-stage.txt")
		try bytes.write(to: unrelated)
		let link = try staging.newFile()
		try FileManager.default.createSymbolicLink(at: link, withDestinationURL: original)
		#expect(try !staging.removeOwned(original))
		try staging.removeAbandoned()
		#expect(!FileManager.default.fileExists(atPath: abandoned.path))
		#expect(!FileManager.default.fileExists(atPath: link.path))
		#expect(try Data(contentsOf: original) == bytes)
		#expect(FileManager.default.fileExists(atPath: unrelated.path))
		let unsafe = AttachmentStagingFiles(temporaryDirectory: root.appendingPathComponent("unsafe-temp"))
		try FileManager.default.createDirectory(
			at: root.appendingPathComponent("unsafe-temp"),
			withIntermediateDirectories: true,
		)
		try FileManager.default.createSymbolicLink(at: unsafe.directory, withDestinationURL: staging.directory)
		var rejected = false
		do { try unsafe.removeAbandoned() } catch { rejected = true }
		#expect(rejected)
		#expect(FileManager.default.fileExists(atPath: unrelated.path))
		try FileManager.default.removeItem(at: root)
	}
}

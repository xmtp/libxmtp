@testable import ExampleCore
import Foundation
import Testing

private final class CopyProbe: @unchecked Sendable {
	private let lock = NSLock()
	private let release = DispatchSemaphore(value: 0)
	private var began = false
	private var main = false
	func beforeWrite() {
		lock.lock()
		if began {
			lock.unlock(); return
		}
		began = true; main = Thread.isMainThread; lock.unlock()
		if !Thread.isMainThread {
			release.wait()
		}
	}

	var started: Bool {
		lock.lock(); defer { lock.unlock() }; return began
	}

	var ranOnMain: Bool {
		lock.lock(); defer { lock.unlock() }; return main
	}

	func resume() {
		release.signal()
	}
}

struct AttachmentFileCopyTests {
	private struct Fixture { let directory: URL; let source: URL; let target: URL }
	private func fixture() throws -> Fixture {
		let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		let source = directory.appendingPathComponent("source")
		try Data(repeating: 42, count: 131_072).write(to: source)
		return Fixture(directory: directory, source: source, target: directory.appendingPathComponent("target"))
	}

	@MainActor @Test func copyingLeavesUIActorFreeAndCancellationRemovesPartialFile() async throws {
		let files = try fixture()
		let directory = files.directory; let source = files.source; let target = files.target
		defer {
			do { try FileManager.default.removeItem(at: directory) } catch { Issue.record("Fixture cleanup failed: \(error)") }
		}
		let probe = CopyProbe()
		let copier = AttachmentFileCopier(beforeWrite: { probe.beforeWrite() })
		let task = Task { try await copier.copy(source, to: target) }
		let deadline = ContinuousClock.now.advanced(by: .seconds(2))
		while !probe.started, ContinuousClock.now < deadline {
			await Task.yield()
		}
		#expect(probe.started)
		#expect(!probe.ranOnMain)
		#expect(FileManager.default.fileExists(atPath: target.path))
		task.cancel(); probe.resume()
		do { try await task.value; Issue.record("Cancelled copy succeeded") } catch is CancellationError {}
		#expect(!FileManager.default.fileExists(atPath: target.path))
		#expect(try Data(contentsOf: source).count == 131_072)
	}

	@Test func exactLimitCopiesAllBytesAndExcessRemovesPartialFile() async throws {
		let files = try fixture()
		let directory = files.directory; let source = files.source; let target = files.target
		defer {
			do { try FileManager.default.removeItem(at: directory) } catch { Issue.record("Fixture cleanup failed: \(error)") }
		}
		let copier = AttachmentFileCopier()
		try await copier.copy(source, to: target, limit: 131_072)
		#expect(try Data(contentsOf: target) == Data(contentsOf: source))
		try FileManager.default.removeItem(at: target)
		do { try await copier.copy(source, to: target, limit: 65536); Issue.record("Oversize copy succeeded")
		} catch AttachmentFileCopier.CopyError.tooLarge {}
		#expect(!FileManager.default.fileExists(atPath: target.path))
	}

	@Test func missingSourceRemovesCreatedTarget() async throws {
		let files = try fixture()
		let directory = files.directory; let source = files.source; let target = files.target
		defer {
			do { try FileManager.default.removeItem(at: directory) } catch { Issue.record("Fixture cleanup failed: \(error)") }
		}
		try FileManager.default.removeItem(at: source)
		do { try await AttachmentFileCopier().copy(source, to: target); Issue.record("Missing source copied") } catch {}
		#expect(!FileManager.default.fileExists(atPath: target.path))
	}
}

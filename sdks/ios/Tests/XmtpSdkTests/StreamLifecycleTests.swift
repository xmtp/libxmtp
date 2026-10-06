import Foundation
#if canImport(UIKit)
	import UIKit
#endif
import XCTest
@testable import XmtpSdk

/// The Apple stream lifecycle manager (`runtime/AppleLifecycle.swift`).
final class StreamLifecycleTests: XCTestCase {
	private enum SuspendFailure: Error { case expected }

	/// A background launch holds every client start until the first suspension ends.
	func testStartupWaitsForTheInitialSuspension() async throws {
		let entered = Shared(false)
		let suspends = Shared(0)
		let returned = Shared(0)
		let (release, signal) = AsyncStream<Void>.makeStream()
		defer { signal.finish() }
		let manager = StreamLifecycleManager(suspend: {
			suspends.update { $0 += 1 }
			entered.update { $0 = true }
			var iterator = release.makeAsyncIterator()
			_ = await iterator.next()
		}, resume: {})
		// Registration selects this state on a background launch.
		manager.setDesired(live: false)
		let starts = (0 ..< 2).map { _ in
			Task {
				await manager.enableIfNeeded()
				returned.update { $0 += 1 }
			}
		}
		let suspending = await eventually(seconds: 2) { entered.value }
		XCTAssertTrue(suspending, "The initial suspension did not start")
		try await pause(seconds: 0.2)
		XCTAssertEqual(returned.value, 0, "A client start returned before the initial suspension ended")
		signal.finish()
		for start in starts {
			await start.value
		}
		XCTAssertEqual(returned.value, 2)
		XCTAssertEqual(suspends.value, 1)
	}

	/// Rust sets its suspend latch before an acknowledgement can fail, so a
	/// failed suspension is still followed by a resume. This holds when the
	/// foreground event arrives during the failed suspension, and after it.
	func testForegroundResumesAfterAFailedSuspension() async {
		for overlap in [false, true] {
			let entered = Shared(false)
			let calls = Shared<[String]>([])
			let (release, signal) = AsyncStream<Void>.makeStream()
			let manager = StreamLifecycleManager(suspend: {
				calls.update { $0.append("suspend") }
				entered.update { $0 = true }
				var iterator = release.makeAsyncIterator()
				_ = await iterator.next()
				throw SuspendFailure.expected
			}, resume: { calls.update { $0.append("resume") } })
			let suspension = manager.setDesired(live: false)
			let suspending = await eventually(seconds: 2) { entered.value }
			XCTAssertTrue(suspending, "The suspension did not start")
			if overlap {
				manager.setDesired(live: true)
			}
			signal.finish()
			await suspension?.value
			if !overlap {
				await manager.setDesired(live: true)?.value
			}
			XCTAssertEqual(calls.value, ["suspend", "resume"], "overlap: \(overlap)")
		}
	}

	/// A real background and foreground cycle with the native suspend and resume
	/// calls. A suspended process receives no network message; after the
	/// foreground event the open stream delivers the message sent meanwhile.
	func testBackgroundSuspendsAndForegroundResumesALiveStream() async throws {
		let manager = StreamLifecycleManager()
		try await checkBackgroundCycle(
			background: { await manager.setDesired(live: false)?.value },
			foreground: { await manager.setDesired(live: true)?.value },
		)
	}

	#if canImport(UIKit)
		/// The UIKit background and foreground notifications drive the shared
		/// manager that client creation registers.
		func testApplicationNotificationsSuspendAndResumeALiveStream() async throws {
			XCTAssertTrue(SDKClient.manageStreamLifecycle)
			try await checkBackgroundCycle(
				background: { await Self.post(UIApplication.didEnterBackgroundNotification) },
				foreground: { await Self.post(UIApplication.willEnterForegroundNotification) },
			)
		}

		/// Posts `name` and waits until the shared manager has applied it.
		private static func post(_ name: Notification.Name) async {
			await MainActor.run { NotificationCenter.default.post(name: name, object: nil) }
			await StreamLifecycleManager.shared.enableIfNeeded()
		}
	#endif

	/// Opens a live stream, runs `background`, sends a message, and runs
	/// `foreground`. The message must arrive only after `foreground`. A message
	/// sent before `background` measures normal delivery, and the suspended
	/// stream is watched for three times that long, at least 3 seconds, so a
	/// slow backend cannot hide a stream that was not suspended. Both clients
	/// end on every exit.
	private func checkBackgroundCycle(
		background: () async -> Void, foreground: () async -> Void,
	) async throws {
		try await withLiveClients(2) { clients in
			try await checkBackgroundCycle(
				receiver: clients[0], sender: clients[1], background: background, foreground: foreground,
			)
		}
	}

	private func checkBackgroundCycle(
		receiver: SDKClient, sender: SDKClient,
		background: () async -> Void, foreground: () async -> Void,
	) async throws {
		let group = try await sender.conversations.createGroup(members: [receiver.inboxId()])
		try await receiver.conversations.sync()
		guard case let .group(joined)? = try await receiver.conversations.getById(id: group.id()) else {
			return XCTFail("The receiver did not join the group")
		}
		let received = Shared<[MessageId]>([])
		let consumer = Task {
			for try await message in try await joined.streamMessages() {
				received.update { $0.append(message.id) }
			}
		}
		defer { consumer.cancel() }
		let sentAt = Date()
		let foregroundId = try await group.sendText(text: "foreground")
		let live = await eventually(seconds: 30) { received.value.contains(foregroundId) }
		guard live else {
			return XCTFail("The stream did not deliver before the background event")
		}
		let window = max(3, 3 * Date().timeIntervalSince(sentAt))

		await background()
		let backgroundId: MessageId
		let deliveredInBackground: Bool
		do {
			backgroundId = try await group.sendText(text: "background")
			deliveredInBackground = await eventually(seconds: window) { received.value.contains(backgroundId) }
		} catch {
			// Resume on every exit, so later tests in this process keep live streams.
			await foreground()
			throw error
		}
		// Resume before any assertion can stop the test.
		await foreground()
		XCTAssertFalse(deliveredInBackground, "A suspended stream received a network message in \(window) seconds")
		let resumed = await eventually(seconds: 60) { received.value.contains(backgroundId) }
		XCTAssertTrue(resumed, "The foreground event did not resume the stream")

		consumer.cancel()
		_ = try? await consumer.value
	}
}

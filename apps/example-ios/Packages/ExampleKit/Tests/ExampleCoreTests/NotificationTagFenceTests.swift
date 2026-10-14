@testable import ExampleCore
import Testing

@MainActor struct NotificationTagFenceTests {
	@Test func removingOneTagPreservesUnrelatedHeldPublication() async {
		let fence = PushSubmissionFence()
		let removed = HeldTagNotification(), unrelated = HeldTagNotification()
		let first = publish("removed", fence: fence, platform: removed)
		let second = publish("unrelated", fence: fence, platform: unrelated)
		await waitForAdd(removed); await waitForAdd(unrelated)
		fence.invalidate(tag: "removed"); removed.remove()
		removed.finish(); unrelated.finish()
		#expect(await first.value == false)
		#expect(await second.value == true)
		#expect(!removed.visible)
		#expect(unrelated.visible)
		#expect(unrelated.removals == 0)
	}

	@Test(arguments: [false, true]) func globalInvalidationRemovesAllHeldPublications(throwsAfterAdd: Bool) async {
		let fence = PushSubmissionFence()
		let first = HeldTagNotification(), second = HeldTagNotification()
		first.throwsAfterAdd = throwsAfterAdd; second.throwsAfterAdd = throwsAfterAdd
		let firstPost = publish("a", fence: fence, platform: first)
		let secondPost = publish("b", fence: fence, platform: second)
		await waitForAdd(first); await waitForAdd(second)
		fence.invalidate(); first.remove(); second.remove()
		first.finish(); second.finish()
		#expect(await firstPost.value == false); #expect(await secondPost.value == false)
		#expect(!first.visible && !second.visible)
		#expect(first.removals == 2 && second.removals == 2)
	}

	@Test func removedTagCannotBeginAddWithOldToken() async {
		let fence = PushSubmissionFence()
		let old = fence.capture(tag: "a"), other = fence.capture(tag: "b")
		fence.invalidate(tag: "a")
		var added = false
		#expect(!fence.isCurrent(old)); #expect(fence.isCurrent(other))
		#expect(await fence.submit(expected: old, add: { added = true }, remove: {}) == false)
		#expect(!added)
		#expect(fence.isCurrent(fence.capture(tag: "a")))
	}

	@Test func globalInvalidationCannotReviveOldTagToken() {
		let fence = PushSubmissionFence()
		fence.invalidate(tag: "a")
		let old = fence.capture(tag: "a")
		fence.invalidate()
		#expect(!fence.isCurrent(old))
		#expect(fence.isCurrent(fence.capture(tag: "a")))
	}

	@Test func failedLateAddForRemovedTagIsCleanedUp() async {
		let fence = PushSubmissionFence(), platform = HeldTagNotification()
		platform.throwsAfterAdd = true
		let pending = publish("a", fence: fence, platform: platform)
		await waitForAdd(platform)
		fence.invalidate(tag: "a"); platform.remove(); platform.finish()
		#expect(await pending.value == false)
		#expect(!platform.visible); #expect(platform.removals == 2)
	}

	private func publish(_ tag: String, fence: PushSubmissionFence, platform: HeldTagNotification) -> Task<Bool, Never> {
		let expected = fence.capture(tag: tag)
		return Task {
			await fence.submit(expected: expected, add: { try await platform.add() }, remove: { platform.remove() })
		}
	}

	private func waitForAdd(_ platform: HeldTagNotification) async {
		while platform.waiter == nil {
			await Task.yield()
		}
	}
}

@MainActor private final class HeldTagNotification {
	var waiter: CheckedContinuation<Void, Never>?
	var visible = false
	var removals = 0
	var throwsAfterAdd = false
	func add() async throws {
		await withCheckedContinuation { waiter = $0 }
		visible = true
		if throwsAfterAdd {
			throw HeldNotificationError.failed
		}
	}

	func finish() {
		let held = waiter; waiter = nil; held?.resume()
	}

	func remove() {
		visible = false; removals += 1
	}
}

private enum HeldNotificationError: Error { case failed }

@testable import ExampleCore
import Testing

@MainActor struct ConversationSubmissionFenceTests {
	@Test func conversationInvalidationPreservesUnrelatedHeldAdds() async {
		let fence = PushSubmissionFence(), first = HeldConversationAdd(), second = HeldConversationAdd()
		let firstToken = fence.capture(tag: "first", conversationID: "a")
		let secondToken = fence.capture(tag: "second", conversationID: "b")
		let firstPost = Task {
			await fence.submit(expected: firstToken, add: { await first.add() }, remove: { first.remove() })
		}
		let secondPost = Task { await fence.submit(
			expected: secondToken,
			add: { await second.add() },
			remove: { second.remove() },
		) }
		while first.waiter == nil || second.waiter == nil {
			await Task.yield()
		}
		fence.invalidate(conversationID: "a")
		first.finish(); second.finish()
		#expect(await firstPost.value == false); #expect(await secondPost.value)
		#expect(!first.visible && second.visible)
	}

	@Test func delayedScopedRemovalKeepsFreshAndUnrelatedNotifications() {
		let fence = PushSubmissionFence()
		fence.invalidate(conversationID: "a")
		let removal = fence.capture(tag: "removal", conversationID: "a")
		#expect(fence.shouldRemoveNotification(conversationID: "a", revision: nil, before: removal))
		#expect(fence.shouldRemoveNotification(conversationID: "a", revision: 0, before: removal))
		#expect(!fence.shouldRemoveNotification(conversationID: "a", revision: removal.conversationRevision, before: removal))
		#expect(!fence.shouldRemoveNotification(conversationID: "b", revision: 0, before: removal))
		#expect(!fence.shouldRemoveNotification(conversationID: nil, revision: nil, before: removal))
		fence.invalidate()
		#expect(!fence.shouldRemoveNotification(conversationID: "a", revision: 0, before: removal))
	}
}

@MainActor private final class HeldConversationAdd {
	var waiter: CheckedContinuation<Void, Never>?
	var visible = false
	func add() async {
		await withCheckedContinuation { waiter = $0 }; visible = true
	}

	func finish() {
		let callback = waiter; waiter = nil; callback?.resume()
	}

	func remove() {
		visible = false
	}
}

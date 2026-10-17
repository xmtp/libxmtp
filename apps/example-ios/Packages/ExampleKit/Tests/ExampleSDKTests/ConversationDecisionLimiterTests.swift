import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing
@testable import XmtpSdk

struct ConversationDecisionLimiterTests {
	@Test(arguments: [false, true]) func decisionLookupSharesFourReadPermits(cancel: Bool) async throws {
		let limiter = AsyncLimiter(), gate = DecisionReadGate()
		let occupied = (0 ..< 4).map { _ in Task { try await limiter.withPermit { await gate.hold() } } }
		await gate.waitUntilHeld()
		let raw = DecisionClient(gate: gate)
		let client = SDKClient(raw, codecs: [])
		let queries = SDKConversationQueries(client: client, limiter: limiter)
		let decision = Task { try await queries.setConsent(id: "request", consent: .allowed) }
		try await Task.sleep(for: .milliseconds(50))
		#expect(await gate.reads == 0)
		if cancel {
			decision.cancel()
		}
		await gate.release()
		for task in occupied {
			try await task.value
		}
		if cancel {
			do {
				try await decision.value
				Issue.record("Canceled decision performed its lookup")
			} catch is CancellationError {} catch { throw error }
			#expect(await gate.reads == 0)
		} else {
			try await decision.value
			#expect(await gate.reads == 1)
		}
		try await client.end()
	}
}

private actor DecisionReadGate {
	var reads = 0
	private var held: [CheckedContinuation<Void, Never>] = []
	func hold() async {
		await withCheckedContinuation { held.append($0) }
	}

	func waitUntilHeld() async {
		while held.count != 4 {
			await Task.yield()
		}
	}

	func release() {
		let pending = held; held.removeAll(); pending.forEach { $0.resume() }
	}

	func read() {
		reads += 1
	}
}

private final class DecisionClient: Client, @unchecked Sendable {
	private let values: DecisionConversations
	init(gate: DecisionReadGate) {
		values = DecisionConversations(gate: gate); super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("Fake has no native handle")
	}

	override func clientKey() -> UInt64 {
		UInt64(truncatingIfNeeded: ObjectIdentifier(self).hashValue)
	}

	override func conversations() -> Conversations {
		values
	}

	override func end() async throws {}
}

private final class DecisionConversations: Conversations, @unchecked Sendable {
	private let gate: DecisionReadGate
	init(gate: DecisionReadGate) {
		self.gate = gate; super.init(noHandle: NoHandle())
	}

	required init(unsafeFromHandle _: UInt64) {
		fatalError("Fake has no native handle")
	}

	override func getById(id _: ConversationId) async throws -> Conversation? {
		await gate.read(); return nil
	}
}

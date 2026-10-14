import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing

private enum IdentityEnrichmentFailure: Error { case offline }

struct SDKHandleEnrichmentTests {
	@Test func failedHandleLookupKeepsConversationState() async throws {
		var record = ConversationRecord(id: "dm", kind: .dm, consent: .denied)
		record.preview = "kept"; record.unread = 7
		let result = try await enrichPeerHandle(record) { throw IdentityEnrichmentFailure.offline }
		#expect(result == record)
	}

	@Test func cancelledHandleLookupRemainsCancelled() async {
		let record = ConversationRecord(id: "dm", kind: .dm)
		await #expect(throws: CancellationError.self) { try await enrichPeerHandle(record) { throw CancellationError() } }
	}
}

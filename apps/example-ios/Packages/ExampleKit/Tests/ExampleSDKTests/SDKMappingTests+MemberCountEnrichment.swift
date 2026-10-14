import ExampleCore
@testable import ExampleSDK
import Foundation
import Testing

private enum MemberLookupFailure: Error { case offline }

struct RoundFifteenMemberCountTests {
	@Test func unavailableMemberCountKeepsRequiredConversationState() async throws {
		var record = ConversationRecord(id: "group", kind: .group, name: "Team", consent: .allowed)
		record.preview = "kept"; record.unread = 7
		let result = try await enrichGroupMemberCount(record) { throw MemberLookupFailure.offline }
		#expect(result == record)
	}

	@Test func availableMemberCountEnrichesConversationState() async throws {
		let record = ConversationRecord(id: "group", kind: .group, name: "Team", consent: .denied)
		let result = try await enrichGroupMemberCount(record) { 42 }
		#expect(result.memberCount == 42)
		#expect(result.name == "Team")
		#expect(result.consent == .denied)
	}

	@Test func memberCountCancellationRemainsCancellation() async {
		let record = ConversationRecord(id: "group", kind: .group)
		await #expect(throws: CancellationError.self) {
			try await enrichGroupMemberCount(record) { throw CancellationError() }
		}
	}
}

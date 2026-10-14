@testable import ExampleCore
import Foundation
import Testing

@MainActor struct PolishRoundThirteenTests {
	@Test func currentNotificationStaysPublished() async {
		let fence = PushSubmissionFence()
		var published = false
		#expect(await fence.submit(
			expected: fence.capture(tag: "message"),
			add: { published = true },
			remove: { published = false },
		))
		#expect(published)
	}

	@Test func uppercaseMapKeyUsesUpdatePermission() throws {
		let field = MetadataField(
			id: 0xC001,
			name: "Map",
			shape: .byteMap,
			entries: [FieldEntry("ab", "01")],
			canInsert: false,
		)
		let action = try MetadataRules.entryAction("AB", field: field)
		#expect(action == .update)
		#expect(field.allows(action))
		#expect(try MetadataRules.entryAction("AC", field: field) == .insert)
	}

	@Test func ownFieldReloadRetriesWithoutLosingEdits() async {
		let queries = ReloadMetadataQueries()
		var jobs: [Task<Void, Never>] = []
		let store = MetadataStore(
			queries: queries,
			origin: DestinationOrigin(session: SessionKey(profileID: UUID(), generation: 1), token: 0),
			admit: { _ in true },
			run: { job in jobs.append(Task { await job() }) },
		)
		store.send(.refresh)
		await jobs.last?.value
		#expect(store.state.error == nil)
		var draft = OwnFieldDraft()
		draft.merge(store.state.snapshot.fields)
		draft.set(queries.field.id, to: .value("Unsaved"))
		store.send(.refresh)
		await jobs.last?.value
		#expect(store.state.error != nil)
		#expect(store.state.snapshot.fields == [queries.field])
		store.send(.refresh)
		await jobs.last?.value
		draft.merge(store.state.snapshot.fields)
		#expect(store.state.error == nil)
		#expect(store.state.snapshot.fields == [queries.field])
		#expect(draft.value(for: queries.field.id) == .value("Unsaved"))
		#expect(draft.changed(queries.field.id))
		#expect(queries.edits == 0)
	}
}

@MainActor private final class ReloadMetadataQueries: MetadataQueries {
	let field = MetadataField(id: 0xC201, name: "Status", shape: .userString, present: true, scalar: "Committed")
	var reads = 0
	var edits = 0
	func load() async throws -> MetadataSnapshot {
		reads += 1
		if reads == 2 || reads == 3 {
			throw MetadataRuleError.changed
		}
		return MetadataSnapshot(fields: [field])
	}

	func edit(_: MetadataEdit, baseline _: MetadataSnapshot) async throws {
		edits += 1
	}
}

@testable import ExampleCore
import Foundation
import Testing

struct MetadataRulesTests {
	@Test func byteLimitsAndHex() throws {
		#expect(try MetadataRules.bytes(String(repeating: "ab", count: 8192)).count == 8192)
		#expect(throws: MetadataRuleError.valueLimit) { try MetadataRules.bytes(String(repeating: "ab", count: 8193)) }
		#expect(throws: MetadataRuleError.hex) { try MetadataRules.bytes("xyz") }
		#expect(try MetadataRules.scalar(String(repeating: "é", count: 4096), shape: .string).utf8.count == 8192)
		#expect(throws: MetadataRuleError.valueLimit) { try MetadataRules.scalar(
			String(repeating: "é", count: 4097),
			shape: .string,
		) }
	}

	@Test func serializedCollectionIncludesOuterPrefix() throws {
		let entries = (0 ..< 7).map { FieldEntry(String(format: "%02x", $0), String(repeating: "ff", count: 8192)) }
		try MetadataRules.collection(entries, shape: .byteMap)
		#expect(throws: MetadataRuleError.collectionLimit) { try MetadataRules.collection(
			entries + [FieldEntry("08", String(repeating: "ff", count: 8192))],
			shape: .byteMap,
		) }
	}

	@Test func changedPolicyAndImmutableBlockWrites() throws {
		let old = MetadataField(id: 0xC001, name: "topic", shape: .string, policy: "Admins")
		var changed = old
		changed.policy = "Denied"
		#expect(throws: MetadataRuleError.changed) { try MetadataRules.unchanged(old, changed) }
		var immutable = MetadataField(id: 0xFD00, name: "creator", shape: .inboxSet)
		immutable.componentPresent = true
		#expect(!immutable.canWrite)
		#expect(throws: MetadataRuleError.immutable) { try MetadataRules.unchanged(immutable, immutable) }
		#expect(throws: MetadataRuleError.unsupported) { try MetadataRules.scalar("x", shape: .unsupported) }
	}

	@Test func ownDraftWritesOnlyIntentionalChanges() throws {
		let absent = MetadataField(id: 0xC201, name: "status", shape: .userString)
		let present = MetadataField(id: 0xC202, name: "device", shape: .userString, present: true, scalar: "Phone")
		var draft = OwnFieldDraft()
		draft.merge([absent, present])
		#expect(draft.changes.isEmpty)
		draft.set(absent.id, to: .value(""))
		#expect(draft.changes.count == 1)
		#expect(try #require(draft.changes[absent.id]) == "")
		draft.set(present.id, to: .absent)
		#expect(draft.changes.count == 2)
		#expect(draft.changes.keys.contains(present.id))
		#expect(try #require(draft.changes[present.id]) == nil)
		draft.merge([absent, present])
		#expect(draft.changes.count == 2)
		var committedEmpty = absent
		committedEmpty.present = true
		var committedAbsent = present
		committedAbsent.present = false
		committedAbsent.scalar = ""
		draft.merge([committedEmpty, committedAbsent])
		#expect(draft.changes.isEmpty)
	}
}

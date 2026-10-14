import Foundation

public enum DetailsFixtures {
	public static var group: ConversationDetails {
		ConversationDetails(
			record: .init(id: "preview-group", kind: .group, name: "Protocol design"),
			description: "Spec work for MLS key package rotation.",
			members: [
				.init(id: "you", handle: "nick#2341", role: .superAdmin),
				.init(id: "priya", handle: "priya", otherHandles: ["priya_s"], role: .admin),
				.init(id: "marco", handle: "marco#0412"),
			],
			policies: PermissionPreset.writes(adminOnly: true),
			preset: "Admins only",
			retentionNs: 86_400_000_000_000,
		)
	}

	public static var fields: MetadataSnapshot {
		MetadataSnapshot(fields: [
			.init(id: 0xC101, name: "Topic", shape: .string, policy: "Admins", present: true,
			      scalar: "MLS key package rotation"),
			.init(
				id: 0xC102,
				name: "Pinned message IDs",
				shape: .byteSet,
				policy: "All members",
				entries: [.init("9a41c2e07fd3b58c")],
			),
			.init(
				id: 0xC201,
				name: "Status",
				shape: .userString,
				policy: "Own entry",
				present: true,
				scalar: "Testing push on iOS",
			),
			.init(id: 0xC202, name: "Device label", shape: .userString, policy: "Own entry"),
		])
	}
}

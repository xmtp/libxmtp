import ExampleCore
import Testing

struct MemberPermissionTests {
	@Test(arguments: [GroupRole.member, .admin, .superAdmin])
	func actionPoliciesAreIndependent(role: GroupRole) {
		let member = GroupMember(id: "target")
		let admin = GroupMember(id: "admin", role: .admin)
		let details = details(role: role, targets: [member, admin], policies: [
			.init(.addAdmin, .superAdmin),
			.init(.removeAdmin, .deny),
			.init(.removeMember, .admin),
		])
		#expect(details.canChangeRole(member, own: "own") == (role == .superAdmin))
		#expect(!details.canChangeRole(admin, own: "own"))
		#expect(details.canRemoveMember(member, own: "own") == (role != .member))
		#expect(details.canRemoveMember(admin, own: "own") == (role != .member))
	}

	@Test(arguments: [GroupPolicy.allow, .admin, .superAdmin, .deny, .custom])
	func knownPoliciesRespectActorRole(policy: GroupPolicy) {
		for role in [GroupRole.member, .admin, .superAdmin] {
			let member = GroupMember(id: "target")
			let admin = GroupMember(id: "admin", role: .admin)
			let details = details(role: role, targets: [member, admin], policies: [
				.init(.addAdmin, policy), .init(.removeAdmin, policy), .init(.removeMember, policy),
			])
			let allowed = policy == .allow || (policy == .admin && role != .member)
				|| (policy == .superAdmin && role == .superAdmin)
			#expect(details.canChangeRole(member, own: "own") == allowed)
			#expect(details.canChangeRole(admin, own: "own") == allowed)
			#expect(details.canRemoveMember(member, own: "own") == allowed)
		}
	}

	@Test func unknownPolicyAndProtectedTargetsStayReadOnly() {
		let own = GroupMember(id: "own", role: .superAdmin)
		let superAdmin = GroupMember(id: "super", role: .superAdmin)
		let member = GroupMember(id: "target")
		let unknown = details(role: .superAdmin, targets: [member], policies: [])
		#expect(!unknown.canChangeRole(member, own: "own"))
		#expect(!unknown.canRemoveMember(member, own: "own"))
		var known = details(role: .superAdmin, targets: [superAdmin, member], policies: [
			.init(.addAdmin, .allow), .init(.removeAdmin, .allow), .init(.removeMember, .allow),
		])
		for target in [own, superAdmin, GroupMember(id: "absent")] {
			#expect(!known.canChangeRole(target, own: "own"))
			#expect(!known.canRemoveMember(target, own: "own"))
		}
		#expect(!known.canChangeRole(member, own: "absent"))
		#expect(!known.canRemoveMember(member, own: "absent"))
		known.record.kind = .dm
		#expect(!known.canChangeRole(member, own: "own"))
		#expect(!known.canRemoveMember(member, own: "own"))
	}

	private func details(
		role: GroupRole,
		targets: [GroupMember],
		policies: [PermissionWrite],
	) -> ConversationDetails {
		ConversationDetails(
			record: .init(id: "group", kind: .group),
			members: [GroupMember(id: "own", role: role)] + targets,
			policies: policies,
		)
	}
}

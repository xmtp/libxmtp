import ExampleCore
@testable import ExampleSDK
import Testing
import XmtpSdk

struct SDKMetadataAuthorizationTests {
	@Test func metadataDeniedOperationsStayReadOnly() {
		let permissions = ComponentPermissions(insert: .base(.allow), update: .base(.deny), delete: .base(.allowIfAdmin))
		let descriptor = MetadataFieldDescriptor(
			field: .init(componentId: 0xC001, name: "test"),
			componentType: .string,
			permissions: permissions,
			isUserField: false,
		)
		let member = metadataField(descriptor, value: .scalar(.string("kept")), role: .member)
		#expect(member.scalar == "kept"); #expect(member.canInsert); #expect(!member.canSet); #expect(!member.allows(.delete))
		let admin = metadataField(descriptor, value: .scalar(.string("kept")), role: .admin)
		#expect(admin.allows(.delete)); #expect(!admin.canSet)
		let allDenied = MetadataFieldDescriptor(
			field: descriptor.field,
			componentType: .string,
			permissions: .init(insert: .base(.deny), update: .base(.deny), delete: .base(.deny)),
			isUserField: false,
		)
		#expect(!metadataField(allDenied, value: .scalar(.string("kept"))).canWrite)
	}

	@Test func compositeMetadataAuthorizationUsesActorRole() {
		let policy = MetadataPolicy.any([.base(.deny), .and([.base(.allowIfAdmin), .base(.allowIfSuperAdmin)])])
		#expect(!metadataAllows(policy, role: .member, ownEntry: false))
		#expect(!metadataAllows(policy, role: .admin, ownEntry: false))
		#expect(metadataAllows(policy, role: .superAdmin, ownEntry: false))
		#expect(metadataAllows(.base(.allowIfSelfOrNonMember), role: .member, ownEntry: true))
		#expect(!metadataAllows(.base(.allowIfSelfOrNonMember), role: .member, ownEntry: false))
	}
}

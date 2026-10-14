import ExampleCore
@testable import ExampleSDK
import Testing
import XmtpSdk

struct SDKGroupMappingTests {
	@Test func permissionMappings() {
		let writes = PermissionPreset.writes(adminOnly: true)
		let expectedKinds: [PermissionUpdateKind] = [
			.addMember,
			.removeMember,
			.addAdmin,
			.removeAdmin,
			.updateMetadata,
			.updateMetadata,
			.updateMetadata,
			.updateMetadata,
			.updateMetadata,
		]
		let fields: [MetadataFieldKind?] = [nil, nil, nil, nil, .name, .description, .imageUrl, .appData, .disappearing]
		for index in writes.indices {
			let mapped = sdkPermission(writes[index])
			#expect(mapped.kind == expectedKinds[index])
			#expect(mapped.policy == (index == 2 || index == 3 ? .superAdmin : .admin))
			#expect(mapped.field == fields[index])
		}
		#expect(sdkPermission(PermissionPreset.writes(adminOnly: false)[0]).policy == .allow)
	}

	@Test func sdkPresetRecognitionIsAuthoritative() {
		#expect(corePreset(.allMembers) == "All members")
		#expect(corePreset(.adminOnly) == "Admins only")
		// A changed minimum-version policy makes the SDK's view custom even
		// when the nine policies shown on this screen still match a preset.
		#expect(corePreset(.custom) == "Custom")
	}

	@Test func metadataShapes() {
		let allow = ComponentPermissions(insert: .base(.allow), update: .base(.allow), delete: .base(.allow))
		struct ShapeCase { let type: MetadataComponentType
			let user: Bool
			let shape: FieldShape
		}
		let cases: [ShapeCase] = [
			.init(type: .string, user: false, shape: .string),
			.init(type: .bytes, user: false, shape: .bytes),
			.init(type: .map(keyType: .bytes, valueType: .bytes), user: false, shape: .byteMap),
			.init(type: .set(keyType: .bytes), user: false, shape: .byteSet),
			.init(type: .set(keyType: .inboxId), user: false, shape: .inboxSet),
			.init(type: .map(keyType: .inboxId, valueType: .string), user: true, shape: .userString),
			.init(type: .map(keyType: .inboxId, valueType: .bytes), user: true, shape: .userBytes),
			.init(type: .string, user: true, shape: .unsupported),
			.init(type: .map(keyType: .bytes, valueType: .string), user: false, shape: .unsupported),
			.init(type: .unknown(tag: 99), user: false, shape: .unsupported),
		]
		for sample in cases {
			#expect(metadataShape(.init(
				field: .init(componentId: 0xC001, name: "test"),
				componentType: sample.type,
				permissions: allow,
				isUserField: sample.user,
			)) == sample.shape)
		}
		#expect(!metadataPolicy(.and([])).1)
		#expect(!metadataPolicy(.base(.unknown(tag: 42))).1)
	}
}

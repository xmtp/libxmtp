import Foundation
import XCTest
import XmtpSdk

/// The metadata-field catalogue through the Swift package. Rust tests cover
/// policies, labels and collections; this test checks that the descriptors,
/// values and typed errors cross the binding.
final class MetadataFieldTests: XCTestCase {
	// verifies: META-069
	func testWellKnownCatalogueFieldsReadAndWrite() async throws {
		let alix = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		let bo = try await SDKClient.create(signer: generateLocalSigner(), options: liveOptions())
		let group = try await alix.conversations().createGroup(members: [bo.inboxId()])
		let groupName = metadataFieldRef(field: .groupName)
		let displayName = metadataFieldRef(field: .userDisplayName)
		XCTAssertEqual(groupName.name, "GROUP_NAME")
		XCTAssertEqual(displayName, MetadataFieldRef(componentId: 0x800C, name: "USER_DISPLAY_NAME"))

		let fields = try await group.metadataFields()
		let described = Dictionary(fields.map { ($0.field, $0) }, uniquingKeysWith: { first, _ in first })
		let name = try XCTUnwrap(described[groupName], "\(fields.map(\.field))")
		XCTAssertEqual(name.componentType, .string)
		XCTAssertFalse(name.isUserField)
		let profile = try XCTUnwrap(described[displayName], "\(fields.map(\.field))")
		XCTAssertEqual(profile.componentType, .map(keyType: .inboxId, valueType: .string))
		XCTAssertTrue(profile.isUserField)

		try await group.updateMetadataField(field: groupName, operation: .replace(.string("Team")))
		try await group.updateUserData(values: [UserFieldUpdate(field: displayName, value: .string("Alix"))])
		try await bo.conversations().sync()
		guard case let .group(boGroup)? = try await bo.conversations().getById(id: group.id()) else {
			return XCTFail("Bo did not join the group")
		}
		try await boGroup.sync()
		let value = try await boGroup.metadataValue(field: groupName)
		XCTAssertEqual(value, .scalar(.string("Team")))
		let profiles = try await boGroup.userData(fields: [displayName], inboxIds: [alix.inboxId()])
		XCTAssertEqual(profiles, [alix.inboxId(): [UserFieldValue(field: displayName, value: .string("Alix"))]])

		do {
			try await boGroup.updateMetadataField(
				field: MetadataFieldRef(componentId: 0xC0FF, name: nil), operation: .replace(.string("x")),
			)
			XCTFail("A field outside the catalogue was written")
		} catch let XmtpError.UnknownField(details) {
			XCTAssertEqual(details.code, "UnknownField")
			XCTAssertEqual(details.category, .input)
		}
		try await bo.end()
		try await alix.end()
	}
}

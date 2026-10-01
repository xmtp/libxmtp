import Foundation
import XmtpSdk

// Alix and Bo read one group through different backend catalogues, so a
// name labels a field for one reader only and the component ID identifies
// it.
private let status: UInt16 = 0xC001
private let nickname: UInt16 = 0xC002
private let topic: UInt16 = 0xC003
private let avatar: UInt16 = 0xC004
private let later: UInt16 = 0xC005
private let byteMap: UInt16 = 0xC006
private let byteSet: UInt16 = 0xC007
private let mapType: MetadataComponentType = .map(keyType: .bytes, valueType: .bytes)
private let setType: MetadataComponentType = .set(keyType: .bytes)
private let displayNameId: UInt16 = 0x800C
private let nicknameType = MetadataComponentType.map(keyType: .inboxId, valueType: .string)

private func field(_ componentId: UInt16, _ name: String? = nil) -> MetadataFieldRef {
    MetadataFieldRef(componentId: componentId, name: name)
}

private func permissions(_ base: MetadataBasePolicy) -> ComponentPermissions {
    ComponentPermissions(insert: .base(base), update: .base(base), delete: .base(base))
}

private func definition(
    _ componentId: UInt16, _ name: String, _ componentType: MetadataComponentType,
    _ base: MetadataBasePolicy, inDms: Bool
) -> ApplicationComponentDefinition {
    ApplicationComponentDefinition(
        componentId: componentId, name: name, componentType: componentType,
        permissions: permissions(base), inGroups: true, inDms: inDms
    )
}

/// `later` has a type tag no SDK knows, so no conversation registers it.
private let alixCatalogue = [
    definition(status, "status", .string, .allow, inDms: true),
    definition(nickname, "nickname", nicknameType, .allowIfSelfOrNonMember, inDms: true),
    definition(topic, "topic", .string, .allowIfAdmin, inDms: true),
    definition(avatar, "avatar", .bytes, .allow, inDms: false),
    definition(byteMap, "byte_map", mapType, .allow, inDms: false),
    definition(byteSet, "byte_set", setType, .allow, inDms: false),
    definition(later, "later", .unknown(tag: 99), .allow, inDms: true),
]

/// Bo's catalogue gives `status` to another field and names STATUS after a
/// well-known field, with a type and policy the group never committed.
private let boCatalogue = [
    definition(status, "GROUP_NAME", .bytes, .deny, inDms: true),
    definition(avatar, "status", .bytes, .allow, inDms: false),
]

private func application(_ labels: [String?]) -> [MetadataFieldDescriptor] {
    let rows: [(UInt16, MetadataComponentType, MetadataBasePolicy, Bool)] = [
        (status, .string, .allow, false),
        (nickname, nicknameType, .allowIfSelfOrNonMember, true),
        (topic, .string, .allowIfAdmin, false),
        (avatar, .bytes, .allow, false),
        (byteMap, mapType, .allow, false),
        (byteSet, setType, .allow, false),
    ]
    return rows.enumerated().map { index, row in
        MetadataFieldDescriptor(
            field: field(row.0, labels[index]), componentType: row.1,
            permissions: permissions(row.2), isUserField: row.3
        )
    }
}

private func client(
    _ catalogue: [ApplicationComponentDefinition], _ options: ClientOptions
) async throws -> SDKClient {
    try await sdkConformanceUseApplicationComponents(components: catalogue)
    do {
        let host = try await SDKClient.create(
            signer: await generateLocalSigner(),
            options: ClientOptions(
                backend: options.backend,
                storage: StorageOptions(location: .inMemory),
                deviceSync: false
            )
        )
        try await sdkConformanceUseApplicationComponents(components: nil)
        return host
    } catch {
        try await sdkConformanceUseApplicationComponents(components: nil)
        throw error
    }
}

private func expect(_ condition: Bool, _ message: @autoclosure () -> String) throws {
    guard condition else { throw ConformanceFailure(message()) }
}

private func expectKind(
    _ code: String, _ category: ErrorCategory, _ body: () async throws -> Void
) async throws {
    do {
        try await body()
    } catch let error as XmtpError {
        let details: ErrorDetails
        switch (code, error) {
        case let ("PermissionDenied", .PermissionDenied(value)),
             let ("DuplicateField", .DuplicateField(value)),
             let ("UnknownField", .UnknownField(value)):
            details = value
        default:
            throw ConformanceFailure("unexpected error \(error)")
        }
        try expect(
            details.code == code && details.category == category && !details.retryable,
            "unexpected error details \(details)"
        )
        return
    }
    throw ConformanceFailure("\(code) was not raised")
}

// verifies: META-069, META-070, META-071, META-072, META-073
func metadataFields(_ options: ClientOptions) async throws {
    let alix = try await client(alixCatalogue, options)
    let bo = try await client(boCatalogue, options)
    try expect(alix.serverConfiguration().applicationComponents == alixCatalogue, "catalogue projection")
    let alixId = alix.inboxId()
    let boId = bo.inboxId()
    let group = try await alix.conversations().createGroup(members: [boId])
    try await bo.conversations().sync()
    guard case let .group(group: boGroup)? = try await bo.conversations().getById(id: group.id()) else {
        throw ConformanceFailure("Bo does not have the group")
    }

    // Descriptors: the committed type and policies with each reader's labels.
    let displayName = metadataFieldRef(field: .userDisplayName)
    let groupName = metadataFieldRef(field: .groupName)
    try expect(displayName == field(displayNameId, "USER_DISPLAY_NAME"), "\(displayName)")
    let alixFields = try await group.metadataFields()
    let wellKnown: [(MetadataFieldRef, Bool)] = [
        (groupName, false),
        (field(0x8005, "GROUP_DESCRIPTION"), false),
        (field(0x8006, "GROUP_IMAGE_URL"), false),
        (field(0x8007, "MESSAGE_DISAPPEAR_FROM_NS"), false),
        (field(0x8008, "MESSAGE_DISAPPEAR_IN_NS"), false),
        (field(0x8009, "APP_DATA"), false),
        (displayName, true),
        (field(0x800D, "GROUP_IMAGE"), false),
    ]
    let described: [(MetadataFieldRef, Bool)] = alixFields.prefix(8).map { ($0.field, $0.isUserField) }
    try expect(
        described.elementsEqual(wellKnown) { $0.0 == $1.0 && $0.1 == $1.1 },
        "\(described)"
    )
    try expect(
        Array(alixFields.dropFirst(8)) == application(["status", "nickname", "topic", "avatar", "byte_map", "byte_set"]),
        "\(alixFields.dropFirst(8))"
    )
    let boFields = try await Array(boGroup.metadataFields().dropFirst(8))
    try expect(boFields == application(["GROUP_NAME", nil, nil, "status", nil, nil]), "\(boFields)")
    try await expect(group.metadataField(name: "status")?.field == field(status, "status"), "alix status")
    try await expect(boGroup.metadataField(name: "status")?.field == field(avatar, "status"), "bo status")
    try await expect(boGroup.metadataField(name: "GROUP_NAME")?.field == groupName, "bo GROUP_NAME")
    try await expect(group.metadataField(name: "later") == nil, "later")

    // Values: request order from one snapshot, each with the reader's label.
    try await boGroup.updateMetadataField(field: field(status), operation: .replace(.string("hello")))
    try await group.sync()
    try await group.updateMetadataField(field: field(avatar, "avatar"), operation: .replace(.bytes(Data([1, 2, 3]))))
    try await group.updateMetadataField(field: groupName, operation: .replace(.string("Team")))
    try await boGroup.sync()
    let values = try await boGroup.metadataValues(
        fields: [field(avatar, "status"), field(status, "status"), groupName, field(topic)]
    )
    try expect(
        values == [
            MetadataFieldValue(field: field(avatar, "status"), value: .scalar(.bytes(Data([1, 2, 3])))),
            MetadataFieldValue(field: field(status, "GROUP_NAME"), value: .scalar(.string("hello"))),
            MetadataFieldValue(field: groupName, value: .scalar(.string("Team"))),
            MetadataFieldValue(field: field(topic), value: nil),
        ],
        "\(values)"
    )
    try await expect(boGroup.metadataValues(fields: []).isEmpty, "empty batch")

    // Profiles: one commit for two fields; a repeat commits nothing.
    let before = try await boGroup.debugInfo().epoch
    try await boGroup.updateUserData(values: [
        UserFieldUpdate(field: displayName, value: .string("Bo")),
        UserFieldUpdate(field: field(nickname), value: .string("B")),
    ])
    try await expect(boGroup.debugInfo().epoch == before + 1, "two fields, one commit")
    try await boGroup.updateUserData(values: [UserFieldUpdate(field: displayName, value: .string("Bo"))])
    try await expect(boGroup.debugInfo().epoch == before + 1, "no-op write committed")
    try await group.sync()
    let boProfile = [
        UserFieldValue(field: displayName, value: .string("Bo")),
        UserFieldValue(field: field(nickname, "nickname"), value: .string("B")),
    ]
    let everyone = try await group.userData(fields: nil, inboxIds: nil)
    try expect(everyone == [alixId: [], boId: boProfile], "\(everyone)")
    let noFields = try await group.userData(fields: [], inboxIds: nil)
    try expect(noFields == [alixId: [], boId: []], "\(noFields)")
    try await expect(group.userData(fields: nil, inboxIds: []).isEmpty, "no inboxes")
    let nicknames = try await group.userData(fields: [field(nickname)], inboxIds: [boId])
    try expect(nicknames == [boId: Array(boProfile.dropFirst())], "\(nicknames)")

    // Denials are typed and commit nothing.
    try await expectKind("PermissionDenied", .conversation) {
        try await boGroup.updateMetadataField(field: field(topic), operation: .replace(.string("x")))
    }
    try await expectKind("DuplicateField", .input) {
        try await boGroup.updateUserData(values: [
            UserFieldUpdate(field: displayName, value: .string("Bobby")),
            UserFieldUpdate(field: displayName, value: nil),
        ])
    }
    try await expect(boGroup.debugInfo().epoch == before + 1, "rejected write committed")
    try await expect(boGroup.metadataValue(field: field(topic)) == nil, "denied value")
    try await expect(boGroup.mapValue(field: displayName, key: .inboxId(boId)) == .string("Bo"), "rejected profile changed")

    // Collection unions preserve nested byte keys and values through the binding.
    let byteKey = FieldKey.bytes(Data([0, 255]))
    try await group.updateMetadataField(field: field(byteMap), operation: .mapDelta([.insert(byteKey, .bytes(Data([0, 128, 255])))]))
    try await expect(group.metadataValue(field: field(byteMap)) == .map([MapEntry(key: byteKey, value: .bytes(Data([0, 128, 255])))]), "byte map insert")
    try await group.updateMetadataField(field: field(byteMap), operation: .mapDelta([.update(byteKey, .bytes(Data([4, 0])))]))
    try await expect(group.mapValue(field: field(byteMap), key: byteKey) == .bytes(Data([4, 0])), "byte map update")
    try await group.updateMetadataField(field: field(byteMap), operation: .mapDelta([.delete(byteKey)]))
    try await expect(group.metadataValue(field: field(byteMap)) == .map([]), "byte map delete")
    try await group.updateMetadataField(field: field(byteSet), operation: .setDelta([.insert(byteKey)]))
    try await expect(group.metadataValue(field: field(byteSet)) == .set([byteKey]), "byte set insert")
    try await group.updateMetadataField(field: field(byteSet), operation: .setDelta([.delete(byteKey)]))
    try await expect(group.metadataValue(field: field(byteSet)) == .set([]), "byte set delete")

    // A DM holds the pair's profiles and its DM fields, never group-only ones.
    let dm = try await alix.conversations().createDm(peer: boId)
    let dmIds = try await dm.metadataFields().map(\.field.componentId)
    try expect(
        dmIds.contains(displayNameId) && dmIds.contains(nickname) && !dmIds.contains(avatar),
        "\(dmIds)"
    )
    try await dm.updateUserData(values: [UserFieldUpdate(field: displayName, value: .string("Alix"))])
    let dmProfiles = try await dm.userData(fields: nil, inboxIds: nil)
    try expect(
        dmProfiles == [alixId: [UserFieldValue(field: displayName, value: .string("Alix"))], boId: []],
        "\(dmProfiles)"
    )
    try await expectKind("UnknownField", .input) {
        try await dm.updateMetadataField(field: field(avatar), operation: .replace(.bytes(Data([1]))))
    }

    try await alix.end()
    try await bo.end()
}

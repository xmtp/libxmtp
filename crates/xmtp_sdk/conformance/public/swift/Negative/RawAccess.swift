import XmtpSdk

/// Apps must use the host Client. The generated Client stays private, and the
/// identity overloads replace the generated identity routes.
func consumeRaw(
    _ client: SDKClient, _ signer: Signer, _ options: ClientOptions, _ group: Group, _ identity: PublicIdentity
) async throws {
    _ = sdkLogSinkHandoff()
    _ = client.raw
    _ = try await Client.create(signer: signer, options: options)
    _ = try await client.conversations().createGroupWithIdentities(members: [identity], options: nil)
    _ = try await group.addMembersByIdentity(members: [identity])
}

import XmtpSdk

/// Apps must use the host Client. The generated Client stays private.
func consumeRaw(_ client: SDKClient, _ signer: Signer, _ options: ClientOptions) async throws {
    _ = client.raw
    _ = try await Client.create(signer: signer, options: options)
}

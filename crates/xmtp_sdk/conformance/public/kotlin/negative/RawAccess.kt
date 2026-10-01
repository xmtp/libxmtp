import uniffi.xmtp_sdk.*

// Apps must use the host Client. The generated Client stays private, and the
// identity overloads replace the generated identity routes.
suspend fun consumeRaw(
    client: SDKClient,
    signer: Signer,
    options: ClientOptions,
    group: Group,
    identity: PublicIdentity,
) {
    println(sdkLogSinkHandoff())
    println(client.raw)
    println(Client.create(signer, options))
    println(client.conversations().createGroupWithIdentities(listOf(identity), null))
    println(group.addMembersByIdentity(listOf(identity)))
}

import uniffi.xmtp_sdk.*

// Apps must use the host Client. The generated Client stays private.
suspend fun consumeRaw(
    client: SDKClient,
    signer: Signer,
    options: ClientOptions,
) {
    println(client.raw)
    println(Client.create(signer, options))
}

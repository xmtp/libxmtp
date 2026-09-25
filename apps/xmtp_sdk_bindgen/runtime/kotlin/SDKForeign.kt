package uniffi.xmtp_sdk

import kotlinx.coroutines.CancellationException

/** Turn every host failure into the error declared by its foreign trait. */
object SDKForeign {
    fun signer(host: Signer): Signer =
        object : Signer {
            override suspend fun identity(): PublicIdentity =
                try {
                    host.identity()
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Throwable) {
                    throw SignerException.Failed()
                }

            override suspend fun kind(): SignerKind =
                try {
                    host.kind()
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Throwable) {
                    throw SignerException.Failed()
                }

            override suspend fun sign(request: SigningRequest): Signature =
                try {
                    host.sign(request)
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Throwable) {
                    throw SignerException.Failed()
                }
        }

    fun credentials(host: CredentialSource): CredentialSource =
        object : CredentialSource {
            override suspend fun credential(): Credential =
                try {
                    host.credential()
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Throwable) {
                    throw CredentialException.Failed()
                }
        }

    fun logSink(host: LogSink): LogSink =
        object : LogSink {
            override fun log(record: LogRecord) {
                try {
                    host.log(record)
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Throwable) {
                    throw LogSinkException.Failed(error.message ?: "log callback failed")
                }
            }
        }

    fun backend(source: BackendSource): BackendSource =
        when (source) {
            is BackendSource.Options -> {
                BackendSource.Options(
                    source.options.copy(
                        credentials = source.options.credentials?.let(::credentials),
                    ),
                )
            }

            is BackendSource.Connected -> {
                source
            }
        }
}

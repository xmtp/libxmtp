package uniffi.xmtp_sdk

import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files

// Each XmtpException variant here lost its last Android check when the Kotlin
// conformance program moved. A real native call lifts each one, so a wrong
// variant index or a dropped detail in the generated error converter fails.
// Rust owns the error rules: xmtp_sdk/src/error/tests.rs,
// xmtp_sdk/src/tests/reader_cursor.rs::delivery_cursor_rejects_invalid_and_foreign_before_open,
// xmtp_sdk/src/tests/storage_layout.rs::unsafe_storage_label_fails_before_any_path_or_request,
// xmtp_sdk/src/tests/signers.rs::pre_authenticate_runs_before_signing_and_propagates_failure.
class TypedErrorTest {
    private inline fun <reified T : XmtpException> expect(
        code: String,
        category: ErrorCategory,
        error: Throwable?,
    ) {
        assertTrue("Expected ${T::class.simpleName}, got $error", error is T)
        val details =
            when (error) {
                is XmtpException.ConfigurationUnavailable -> error.v1
                is XmtpException.StorageLocation -> error.v1
                is XmtpException.CallbackFailed -> error.v1
                is XmtpException.InvalidCursor -> error.v1
                is XmtpException.ForeignCursor -> error.v1
                is XmtpException.UnknownField -> error.v1
                else -> throw AssertionError("unexpected error $error")
            }
        assertEquals(code, details.code)
        assertEquals(category, details.category)
    }

    @Test
    fun clientSetupErrorsKeepTheirVariant() =
        runBlocking {
            withTimeout(60_000) {
                val unavailable =
                    runCatching {
                        // Nothing listens on port 1.
                        SDKClient.fetchServerConfiguration(
                            BackendSource.Options(BackendOptions(url = "http://127.0.0.1:1")),
                        )
                    }.exceptionOrNull()
                expect<XmtpException.ConfigurationUnavailable>(
                    "ConfigurationUnavailable",
                    ErrorCategory.CONFIGURATION,
                    unavailable,
                )

                val root = Files.createTempDirectory("xmtp-typed-errors-")
                try {
                    val unsafe =
                        runCatching {
                            withClients {
                                create(
                                    options =
                                        liveOptions().copy(
                                            storage =
                                                StorageOptions(
                                                    StorageLocation.Directory(root.toString()),
                                                    label = "bad/name",
                                                ),
                                        ),
                                )
                            }
                        }.exceptionOrNull()
                    expect<XmtpException.StorageLocation>("StorageLocation", ErrorCategory.STORAGE, unsafe)
                    assertFalse((unsafe as XmtpException.StorageLocation).v1.retryable)
                } finally {
                    root.toFile().deleteRecursively()
                }

                val failing =
                    object : PreAuthenticate {
                        override suspend fun run(): Unit = throw PreAuthenticateException.Failed()
                    }
                val callback =
                    runCatching {
                        withClients { create(options = liveOptions().copy(handlers = ClientHandlers(failing))) }
                    }.exceptionOrNull()
                expect<XmtpException.CallbackFailed>("CallbackFailed", ErrorCategory.CALLBACK, callback)
            }
        }

    @Test
    fun readerAndFieldErrorsKeepTheirVariant() =
        runBlocking {
            withTimeout(60_000) {
                withClients {
                    val alix = create()
                    val other = create()
                    val group = alix.conversations().createGroup(emptyList())
                    val invalid =
                        runCatching {
                            group.messageReader(
                                ConversationMessageReaderOptions(from = "invalid"),
                            )
                        }.exceptionOrNull()
                    expect<XmtpException.InvalidCursor>("InvalidCursor", ErrorCategory.STREAM, invalid)
                    val foreign = other.conversations().beginningDeliveryCursor()
                    val wrongClient =
                        runCatching {
                            group.messageReader(
                                ConversationMessageReaderOptions(from = foreign),
                            )
                        }.exceptionOrNull()
                    expect<XmtpException.ForeignCursor>("ForeignCursor", ErrorCategory.STREAM, wrongClient)
                    val unknown =
                        runCatching {
                            group.updateMetadataField(
                                MetadataFieldRef(0xC0FFu, null),
                                ComponentMutation.Replace(FieldValue.String("x")),
                            )
                        }.exceptionOrNull()
                    expect<XmtpException.UnknownField>("UnknownField", ErrorCategory.INPUT, unknown)
                }
            }
        }
}

package org.xmtp.android.example.messenger
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.xmtp.android.example.BuildConfig
import uniffi.xmtp_sdk.AndroidStreamLifecycle
import uniffi.xmtp_sdk.inboxId
import uniffi.xmtp_sdk.resumeStreams
import java.io.File
import java.net.URI

@RunWith(AndroidJUnit4::class)
class SessionLifecycleInstrumentedTest {
    private val context get() =
        InstrumentationRegistry
            .getInstrumentation()
            .targetContext.applicationContext

    @Test fun persistedSignerSignOutAndColdResetKeepProfileBoundaries() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            try {
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        true,
                    )
                val first =
                    checkNotNull(
                        session.active.value,
                    )
                val wallet =
                    checkNotNull(
                        session.secrets
                            .read(
                                first.key.profileId,
                                "wallet",
                            ),
                    )
                val inbox =
                    first.client
                        .inboxId()
                session.preferences
                    .saveDraft(
                        first.key.profileId,
                        SendDraftRef(
                            "unknown-send",
                            "chat",
                            phase =
                                SendPhase.QUEUEING,
                        ),
                    )
                val unrelated =
                    File(
                        context.filesDir,
                        "unrelated-reset-proof",
                    ).apply {
                        writeText("retained")
                    }
                val staged =
                    File(
                        first.paths.temp,
                        "staged",
                    ).apply {
                        parentFile
                            .mkdirs()
                        writeText("plaintext")
                    }
                val downloaded =
                    File(
                        first.paths.attachments,
                        "downloaded",
                    ).apply {
                        writeText("plaintext")
                    }
                val exported =
                    File(
                        first.paths.exports,
                        "exported",
                    ).apply {
                        parentFile
                            .mkdirs()
                        writeText("plaintext")
                    }
                session.unregisterNotifications = {
                    error("Unregister network failure")
                }
                session
                    .signOut()
                assertFalse(
                    session.preferences
                        .signedIn(),
                )
                assertNull(
                    session.active.value,
                )
                assertFalse(
                    session
                        .accepts(
                            first.key,
                        ),
                )
                assertTrue(
                    first.paths.database
                        .exists(),
                )
                assertNull(
                    session.secrets
                        .read(
                            first.key.profileId,
                            "credential",
                        ),
                )
                assertArrayEquals(
                    wallet,
                    session.secrets
                        .read(
                            first.key.profileId,
                            "wallet",
                        ),
                )
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        true,
                    )
                assertEquals(
                    inbox,
                    checkNotNull(
                        session.active.value,
                    ).client
                        .inboxId(),
                )
                assertEquals(
                    1,
                    session.preferences
                        .drafts(
                            first.key.profileId,
                        ).count {
                            it.phase ==
                                SendPhase
                                    .QUEUEING && it.acceptedMessageId == null
                        },
                )
                val otherBackend = "http://127.0.0.1:${URI(BuildConfig.XMTP_BACKEND_URL).port}"
                session
                    .connect(
                        otherBackend,
                        "",
                        true,
                    )
                val otherProfile =
                    checkNotNull(
                        session.active.value,
                    )
                val otherInbox =
                    otherProfile.client
                        .inboxId()
                assertNotEquals(
                    inbox,
                    otherInbox,
                )
                session.preferences
                    .saveMarker(
                        otherProfile.key.profileId,
                        "retained-marker",
                        101,
                    )
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        true,
                    )
                // Simulate process death in STOPPING before native deletion. No handle is restored.
                session
                    .signOut()
                session.preferences
                    .saveReset(
                        first.paths
                            .resetRecord(
                                first.key.profileId,
                            ),
                    )
                val recreated = AppSession(context)
                recreated
                    .restore()
                assertNull(
                    recreated.active.value,
                )
                assertNull(
                    recreated.preferences
                        .reset(),
                )
                assertFalse(
                    first.paths.database
                        .exists(),
                )
                assertFalse(
                    staged
                        .exists(),
                )
                assertFalse(
                    downloaded
                        .exists(),
                )
                assertFalse(
                    exported
                        .exists(),
                )
                assertTrue(
                    unrelated
                        .exists(),
                )
                unrelated
                    .delete()
                assertFalse(
                    recreated.preferences
                        .profiles()
                        .any {
                            it.id ==
                                first.key.profileId
                        },
                )
                assertTrue(
                    otherProfile.paths.database
                        .exists(),
                )
                assertNotNull(
                    recreated.secrets
                        .read(
                            otherProfile.key.profileId,
                            "wallet",
                        ),
                )
                assertEquals(
                    101L,
                    recreated.preferences
                        .marker(
                            otherProfile.key.profileId,
                            "retained-marker",
                        ).insertedAtNs,
                )
                session
                    .connect(
                        otherBackend,
                        "",
                        true,
                    )
                assertEquals(
                    otherInbox,
                    checkNotNull(
                        session.active.value,
                    ).client
                        .inboxId(),
                )
                session
                    .deleteAccount()
            } finally {
                withContext(NonCancellable) {
                    session
                        .signOut()
                }
                AndroidStreamLifecycle.enabled = true
            }
        }

    @Test fun resetFailureAfterDatabaseRemovalRetainsPhaseAndCanResume() =
        runBlocking {
            AndroidStreamLifecycle.enabled = false
            resumeStreams()
            val session = AppSession(context)
            var owned: File? = null
            try {
                session
                    .connect(
                        BuildConfig.XMTP_BACKEND_URL,
                        "",
                        true,
                    )
                val owner =
                    checkNotNull(
                        session.active.value,
                    )
                owned =
                    owner.paths.root
                session
                    .signOut()
                check(
                    owner.paths.database
                        .delete(),
                )
                // Death after the delete and before the phase write still has STOPPING.
                val record =
                    owner.paths
                        .resetRecord(
                            owner.key.profileId,
                        )
                session.preferences
                    .saveReset(record)
                check(
                    owner.paths.root
                        .setReadable(
                            false,
                            false,
                        ),
                )
                check(
                    owner.paths.root
                        .setExecutable(
                            false,
                            false,
                        ),
                )
                val recreated = AppSession(context)
                assertTrue(
                    runCatching {
                        recreated
                            .restore()
                    }.isFailure,
                )
                assertNotNull(
                    recreated.preferences
                        .reset(),
                )
                // A recorded DATABASE_REMOVED phase must also retain a file-cleanup failure.
                recreated.preferences
                    .saveReset(
                        record
                            .copy(
                                phase =
                                    ResetPhase.DATABASE_REMOVED,
                            ),
                    )
                assertTrue(
                    runCatching {
                        recreated
                            .restore()
                    }.isFailure,
                )
                assertEquals(
                    ResetPhase.DATABASE_REMOVED,
                    recreated.preferences
                        .reset()
                        ?.phase,
                )
                check(
                    owner.paths.root
                        .setReadable(
                            true,
                            true,
                        ),
                )
                check(
                    owner.paths.root
                        .setExecutable(
                            true,
                            true,
                        ),
                )
                recreated
                    .restore()
                assertNull(
                    recreated.preferences
                        .reset(),
                )
                assertFalse(
                    owner.paths.root
                        .exists(),
                )
            } finally {
                owned?.setReadable(
                    true,
                    true,
                )
                owned?.setExecutable(
                    true,
                    true,
                )
                withContext(NonCancellable) {
                    session
                        .signOut()
                    if (session.preferences
                            .reset() != null
                    ) {
                        session
                            .deleteAccount()
                    }
                }
                AndroidStreamLifecycle.enabled = true
            }
        }
}

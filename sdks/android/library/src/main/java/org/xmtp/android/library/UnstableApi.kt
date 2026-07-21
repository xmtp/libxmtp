package org.xmtp.android.library

/**
 * Opt-in marker for pre-release ("unstable") APIs. The API shape may
 * still change and, in some cases, the effect is one-way and
 * irreversible. Call sites must acknowledge with
 * `@OptIn(UnstableApi::class)`; when an API graduates the marker is
 * dropped from it and — once the marker itself is removed — the opt-in
 * fails to compile, forcing a migration to the stable form.
 */
@RequiresOptIn(level = RequiresOptIn.Level.ERROR)
@Retention(AnnotationRetention.BINARY)
@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION, AnnotationTarget.PROPERTY)
annotation class UnstableApi(
    val message: String,
)

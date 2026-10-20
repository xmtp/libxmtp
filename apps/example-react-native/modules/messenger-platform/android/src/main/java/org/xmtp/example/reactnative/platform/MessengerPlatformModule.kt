package org.xmtp.example.reactnative.platform

import android.content.pm.ApplicationInfo
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

// The launch variables the app reads. `adb shell am start --es <NAME> <value>`
// passes them as launch intent extras. Any app can start the exported
// activity with extras, so only a debuggable build reads them: a build that
// can ship never takes its backend address or the private attachment opt-in
// from another app.
private val launchKeys =
    listOf(
        "XMTP_BACKEND_URL",
        "XMTP_ALLOW_PRIVATE_ATTACHMENTS",
        "XMTP_UI_TEST_ATTACHMENT",
    )

class MessengerPlatformModule : Module() {
    override fun definition() =
        ModuleDefinition {
            Name("MessengerPlatform")

            Function("launchEnvironment") {
                val context = appContext.reactContext ?: throw Exceptions.ReactContextLost()
                val debuggable = (context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
                val extras = if (debuggable) appContext.currentActivity?.intent?.extras else null
                launchKeys.mapNotNull { key -> extras?.getString(key)?.let { key to it } }.toMap()
            }

            // App-private internal storage: credential-encrypted and, with
            // android:allowBackup="false", never backed up.
            Function("applicationSupportDirectory") {
                val context = appContext.reactContext ?: throw Exceptions.ReactContextLost()
                context.filesDir.absolutePath
            }

            // No-ops: the manifest disables backup and internal storage is
            // already credential-encrypted.
            Function("excludeFromBackup") { _: String -> }
            Function("protectUntilFirstUserAuthentication") { _: String -> }

            // Android has no navigation subtitle; the view renders nothing.
            View(NavigationSubtitleView::class) {
                Prop("subtitle") { _: NavigationSubtitleView, _: String? -> }
                Prop("largeSubtitle") { _: NavigationSubtitleView, _: String? -> }
            }
        }
}

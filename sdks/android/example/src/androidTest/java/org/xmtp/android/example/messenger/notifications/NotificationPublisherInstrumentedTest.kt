package org.xmtp.android.example.messenger.notifications

import android.Manifest
import android.app.Notification
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assume.assumeTrue
import org.junit.Test

class NotificationPublisherInstrumentedTest {
    @Test fun configuredPublisherRejectsDeniedAndroidPermission() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(Build.VERSION.SDK_INT >= 33)
        val permissions = context.packageManager.getPackageInfo(context.packageName, PackageManager.GET_PERMISSIONS)
        assumeTrue(permissions.requestedPermissions?.contains(Manifest.permission.POST_NOTIFICATIONS) == true)
        assumeTrue(
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
                PackageManager.PERMISSION_GRANTED,
        )
        val notification =
            Notification
                .Builder(
                    context,
                    "messages",
                ).setSmallIcon(android.R.drawable.ic_dialog_email)
                .build()
        assertFalse(NotificationPublisher.post(context, "permission-test", notification))
    }
}

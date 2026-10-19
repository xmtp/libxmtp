package com.xmtp.consumer.fixture

import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod

/** Reports the raw OS sandbox root, separate from SDK path policy. */
class XmtpMigrationFixture(
    context: ReactApplicationContext,
) : ReactContextBaseJavaModule(context) {
    override fun getName() = "XmtpMigrationFixture"

    @ReactMethod
    fun osStorage(promise: Promise) {
        promise.resolve(
            Arguments.createMap().apply {
                putString("filesDirectory", reactApplicationContext.filesDir.absolutePath)
            },
        )
    }
}

class XmtpMigrationFixturePackage : com.facebook.react.ReactPackage {
    override fun createNativeModules(context: ReactApplicationContext): List<com.facebook.react.bridge.NativeModule> =
        listOf(XmtpMigrationFixture(context))

    override fun createViewManagers(
        context: ReactApplicationContext,
    ): List<com.facebook.react.uimanager.ViewManager<*, *>> = emptyList()
}

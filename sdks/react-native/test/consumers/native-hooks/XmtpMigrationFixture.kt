package com.xmtp.consumer.fixture

import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.ReactContextBaseJavaModule
import com.facebook.react.bridge.ReactMethod
import com.facebook.react.bridge.WritableMap
import java.io.File
import java.security.MessageDigest

/** Native file operations for immutable test inputs, separate from SDK policy. */
class XmtpMigrationFixture(
    context: ReactApplicationContext,
) : ReactContextBaseJavaModule(context) {
    private val directory = File(context.filesDir, "xmtp-migration-fixture")

    private fun database(unsupported: Boolean = false) =
        File(directory, if (unsupported) "unsupported-schema.db3" else "react-native-5.7.0.db3")

    override fun getName() = "XmtpMigrationFixture"

    private fun state(unsupported: Boolean = false): WritableMap =
        Arguments.createMap().apply {
            for (suffix in listOf("", ".sqlcipher_salt", "-wal", "-shm", "-journal")) {
                val file = File(database(unsupported).path + suffix)
                if (!file.exists()) {
                    putNull(suffix)
                } else {
                    val digest = MessageDigest.getInstance("SHA-256")
                    file.inputStream().use { stream ->
                        val buffer = ByteArray(65536)
                        while (true) {
                            val count = stream.read(buffer)
                            if (count < 0) break
                            digest.update(buffer, 0, count)
                        }
                    }
                    putString(suffix, digest.digest().joinToString("") { "%02x".format(it) })
                }
            }
        }

    @ReactMethod
    fun osStorage(promise: Promise) {
        promise.resolve(
            Arguments.createMap().apply {
                putString("filesDirectory", reactApplicationContext.filesDir.absolutePath)
            },
        )
    }

    @ReactMethod
    fun exists(
        path: String,
        promise: Promise,
    ) {
        promise.resolve(File(path).exists())
    }

    @ReactMethod
    fun readFile(
        path: String,
        promise: Promise,
    ) {
        try {
            val file = File(path)
            check(file.isAbsolute && file.length() <= 32 * 1024 * 1024) { "fixture file must be absolute and bounded" }
            val bytes = Arguments.createArray()
            for (byte in file.readBytes()) bytes.pushInt(byte.toInt() and 255)
            promise.resolve(bytes)
        } catch (error: Exception) {
            promise.reject("FIXTURE_FILE", "Could not read native fixture file", error)
        }
    }

    @ReactMethod
    fun prepare(promise: Promise) {
        try {
            check(!directory.exists()) { "migration fixture may be prepared only once per clean install" }
            check(directory.mkdirs())
            for (name in listOf(
                "react-native-5.7.0.db3",
                "react-native-5.7.0.db3.sqlcipher_salt",
                "unsupported-schema.db3",
                "unsupported-schema.db3.sqlcipher_salt",
            )) {
                reactApplicationContext.assets.open("xmtp-legacy/$name").use { source ->
                    File(directory, name).outputStream().use { source.copyTo(it) }
                }
            }

            fun asset(name: String) =
                reactApplicationContext.assets.open("xmtp-legacy/$name").bufferedReader().use {
                    it.readText()
                }
            promise.resolve(
                Arguments.createMap().apply {
                    putString("databasePath", database().path)
                    putString("outputPath", File(directory, "converted.xmtp").path)
                    putString("expectedJson", asset("expected-records.json"))
                    putString("inspectionJson", asset("source-inspection.json"))
                    putString("unsupportedDatabasePath", database(true).path)
                    putString("unsupportedJson", asset("unsupported-schema.json"))
                    putMap("sourceState", state())
                    putMap("unsupportedState", state(true))
                },
            )
        } catch (
            error: Exception,
        ) {
            promise.reject("FIXTURE_PREPARE", "Could not prepare immutable legacy fixture", error)
        }
    }

    @ReactMethod
    fun sourceState(
        unsupported: Boolean,
        promise: Promise,
    ) {
        try {
            promise.resolve(state(unsupported))
        } catch (
            error: Exception,
        ) {
            promise.reject("FIXTURE_HASH", "Could not inspect legacy source files", error)
        }
    }
}

class XmtpMigrationFixturePackage : com.facebook.react.ReactPackage {
    override fun createNativeModules(context: ReactApplicationContext): List<com.facebook.react.bridge.NativeModule> =
        listOf(XmtpMigrationFixture(context))

    override fun createViewManagers(
        context: ReactApplicationContext,
    ): List<com.facebook.react.uimanager.ViewManager<*, *>> = emptyList()
}

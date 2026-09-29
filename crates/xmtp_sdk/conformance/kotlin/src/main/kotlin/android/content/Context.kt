package android.content

import java.io.File

/** JVM stand-in for the Android files directory in the storage contract test. */
abstract class Context {
    abstract val filesDir: File
}

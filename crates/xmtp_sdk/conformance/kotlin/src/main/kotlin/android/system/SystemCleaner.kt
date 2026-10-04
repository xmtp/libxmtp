package android.system

import java.lang.ref.Cleaner

/** Host conformance must use the JNA cleaner. */
object SystemCleaner {
    fun cleaner(): Cleaner = error("Host conformance selected Android SystemCleaner")
}

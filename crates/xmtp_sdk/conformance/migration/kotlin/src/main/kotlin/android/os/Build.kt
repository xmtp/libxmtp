package android.os

// Select the normal JNA cleaner when this consumer runs on the host JVM.
object Build {
    @Suppress("ktlint:standard:class-naming")
    object VERSION {
        const val SDK_INT = 0
    }

    @Suppress("ktlint:standard:class-naming")
    object VERSION_CODES {
        const val UPSIDE_DOWN_CAKE = 34
    }
}

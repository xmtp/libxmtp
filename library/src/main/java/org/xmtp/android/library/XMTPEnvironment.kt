package org.xmtp.android.library

enum class XMTPEnvironment(val rawValue: String) {
    DEV("grpc.dev.xmtp.network"),
    PRODUCTION("grpc.production.xmtp.network"),
    LOCAL("10.0.2.2") {
        override fun withValue(value: String): XMTPEnvironment {
            return LOCAL.apply { customValue = value }
        }
    };

    private var customValue: String = ""

    open fun withValue(value: String): XMTPEnvironment {
        return this
    }

    companion object {
        operator fun invoke(rawValue: String): XMTPEnvironment {
            return XMTPEnvironment.values().firstOrNull { it.rawValue == rawValue }
                ?: LOCAL.withValue(rawValue)
        }
    }

    // This function returns the actual raw value for the enum, handling the CUSTOM case.
    fun getValue(): String {
        return if (this == LOCAL && customValue.isNotEmpty()) customValue else rawValue
    }

    fun getUrl(): String {
        return when (this) {
            DEV -> "https://${getValue()}:443"
            PRODUCTION -> "https://${getValue()}:443"
            LOCAL -> "http://${getValue()}:5556"
        }
    }
}

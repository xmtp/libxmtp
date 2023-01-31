package org.xmtp.android.library

enum class XMTPEnvironment(val rawValue: String) {
    DEV("dev.xmtp.network"),
    PRODUCTION("production.xmtp.network"),
    LOCAL("10.0.2.2"),
    ;

    companion object {
        operator fun invoke(rawValue: String) =
            XMTPEnvironment.values().firstOrNull { it.rawValue == rawValue }
    }
}

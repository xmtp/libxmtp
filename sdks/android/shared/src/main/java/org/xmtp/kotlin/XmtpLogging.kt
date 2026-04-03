package org.xmtp.kotlin

object XmtpLogging {
    @Volatile
    var logger: XmtpLogger = NoOpLogger
}

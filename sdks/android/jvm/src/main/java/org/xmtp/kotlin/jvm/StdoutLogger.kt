package org.xmtp.kotlin.jvm

import org.xmtp.kotlin.XmtpLogger

class StdoutLogger : XmtpLogger {
    override fun debug(tag: String, message: String) {
        println("D/$tag: $message")
    }

    override fun warning(tag: String, message: String, throwable: Throwable?) {
        System.err.println("W/$tag: $message")
        throwable?.printStackTrace(System.err)
    }

    override fun error(tag: String, message: String, throwable: Throwable?) {
        System.err.println("E/$tag: $message")
        throwable?.printStackTrace(System.err)
    }
}

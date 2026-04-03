package org.xmtp.kotlin

interface XmtpLogger {
    fun debug(tag: String, message: String)

    fun warning(tag: String, message: String, throwable: Throwable? = null)

    fun error(tag: String, message: String, throwable: Throwable? = null)
}

object NoOpLogger : XmtpLogger {
    override fun debug(tag: String, message: String) {}

    override fun warning(tag: String, message: String, throwable: Throwable?) {}

    override fun error(tag: String, message: String, throwable: Throwable?) {}
}

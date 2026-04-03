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

object StdoutLogger : XmtpLogger {
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

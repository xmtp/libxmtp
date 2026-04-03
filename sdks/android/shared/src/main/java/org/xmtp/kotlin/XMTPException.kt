package org.xmtp.kotlin

class XMTPException(
    message: String,
    exception: java.lang.Exception? = null,
) : Exception(message, exception)

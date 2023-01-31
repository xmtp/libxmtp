package org.xmtp.android.library.extensions

import java.util.Date

val Date.millisecondsSinceEpoch: Double
    get() = (System.currentTimeMillis() * 1000).toDouble()

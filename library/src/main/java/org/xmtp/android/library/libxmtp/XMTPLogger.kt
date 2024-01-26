package org.xmtp.android.library.libxmtp

import android.util.Log
import uniffi.xmtpv3.FfiLogger

class XMTPLogger : FfiLogger {
    override fun log(level: UInt, levelLabel: String, message: String) {
        when (level.toInt()) {
            1 -> Log.e(levelLabel, message)
            2 -> Log.w(levelLabel, message)
            3 -> Log.i(levelLabel, message)
            4 -> Log.d(levelLabel, message)
            5 -> Log.v(levelLabel, message)
            else -> Log.i("$level $levelLabel", message)
        }
    }
}

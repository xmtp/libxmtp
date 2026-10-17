package org.xmtp.android.library

import uniffi.xmtp_sdk.*

fun localApi(appVersion: String? = null): BackendOptions =
    BackendOptions(url = BuildConfig.XMTP_BACKEND_URL, appVersion = appVersion)

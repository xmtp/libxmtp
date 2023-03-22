package org.xmtp.android.example.utils

import android.accounts.AccountManager
import android.content.Context
import org.xmtp.android.example.R

class KeyUtil(val context: Context) {
    fun loadKeys(): String? {
        val accountManager = AccountManager.get(context)
        val accounts =
            accountManager.getAccountsByType(context.getString(R.string.account_type))
        val account = accounts.firstOrNull() ?: return null
        return accountManager.getPassword(account)
    }
}
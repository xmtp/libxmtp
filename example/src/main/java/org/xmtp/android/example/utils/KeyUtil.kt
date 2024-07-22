package org.xmtp.android.example.utils

import android.accounts.AccountManager
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64.NO_WRAP
import android.util.Base64.decode
import android.util.Base64.encodeToString
import org.xmtp.android.example.R
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey


class KeyUtil(val context: Context) {
    private val PREFS_NAME = "EncryptionPref"
    fun loadKeys(): String? {
        val accountManager = AccountManager.get(context)
        val accounts =
            accountManager.getAccountsByType(context.getString(R.string.account_type))
        val account = accounts.firstOrNull() ?: return null
        return accountManager.getPassword(account)
    }

    fun storeKey(address: String, key: ByteArray?) {
        val alias = "xmtp-dev-${address.lowercase()}"

        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val editor = prefs.edit()
        editor.putString(alias, encodeToString(key, NO_WRAP))
        editor.apply()
    }

    fun retrieveKey(address: String): ByteArray? {
        val alias = "xmtp-dev-${address.lowercase()}"

        val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val keyString = prefs.getString(alias, null)
        return if (keyString != null) {
            decode(keyString, NO_WRAP)
        } else null
    }
}
package org.xmtp.android.library.push

import android.content.Context
import android.content.Context.MODE_PRIVATE
import android.content.SharedPreferences

class PushPreferences private constructor() {
    companion object {
        private const val PUSH_PREFERENCES = "push_preferences"
        private const val KEY_INSTALLATION_ID = "installation_id"

        private fun pushPreferences(context: Context): SharedPreferences =
            context.getSharedPreferences(PUSH_PREFERENCES, MODE_PRIVATE)

        fun clearAll(context: Context) {
            pushPreferences(context).edit().clear().apply()
        }

        fun getInstallationId(context: Context): String? {
            return pushPreferences(context)
                .getString(KEY_INSTALLATION_ID, "")
        }

        fun setInstallationId(context: Context, id: String) {
            pushPreferences(context)
                .edit().putString(KEY_INSTALLATION_ID, id).apply()
        }
    }
}

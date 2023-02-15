package org.xmtp.android.example

import android.content.Context

object UserPreferences {
    private const val USER_PREFERENCES = "user_preferences"

    private const val KEY_SIGNED_IN_ADDRESS = "key_signed_in_address"

    private fun userPreferences(context: Context) =
        context.getSharedPreferences(USER_PREFERENCES, Context.MODE_PRIVATE)

    fun signedInAddress(context: Context): String? =
        userPreferences(context).getString(KEY_SIGNED_IN_ADDRESS, null)

    fun setSignedInAddress(context: Context, address: String?) {
        with(userPreferences(context).edit()) {
            putString(KEY_SIGNED_IN_ADDRESS, address)
            apply()
        }
    }
}

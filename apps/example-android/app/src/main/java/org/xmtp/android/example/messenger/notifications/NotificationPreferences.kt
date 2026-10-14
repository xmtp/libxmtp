package org.xmtp.android.example.messenger.notifications

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first

private val Context.notificationStore by preferencesDataStore("messenger-notifications")

class NotificationPreferences(
    context: Context,
) {
    private val store = context.applicationContext.notificationStore

    suspend fun enabled(profile: String) = store.data.first()[booleanPreferencesKey("$profile/enabled")] ?: false

    suspend fun setEnabled(
        profile: String,
        enabled: Boolean,
        admit: (() -> Unit) -> Boolean = { change ->
            change()
            true
        },
    ): Boolean {
        var accepted = false
        store.edit { prefs -> accepted = admit { prefs[booleanPreferencesKey("$profile/enabled")] = enabled } }
        return accepted
    }

    suspend fun muted(
        profile: String,
        conversation: String,
    ) = store.data.first()[booleanPreferencesKey("$profile/mute/$conversation")] ?: false

    suspend fun setMuted(
        profile: String,
        conversation: String,
        muted: Boolean,
        admit: (() -> Unit) -> Boolean = { change ->
            change()
            true
        },
    ): Boolean {
        var accepted = false
        store.edit { prefs -> accepted = admit { prefs[booleanPreferencesKey("$profile/mute/$conversation")] = muted } }
        return accepted
    }

    suspend fun remove(profile: String) {
        store.edit { prefs ->
            prefs
                .asMap()
                .keys
                .filter {
                    it.name.startsWith(
                        "$profile/",
                    )
                }.forEach { prefs.remove(it) }
        }
    }
}

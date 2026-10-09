package org.xmtp.android.example.messenger.notifications

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.first

private val Context.notificationStore by preferencesDataStore("messenger-notifications")

class NotificationPreferences(context: Context) {
    private val store = context.applicationContext.notificationStore
    suspend fun enabled(profile: String) = store.data.first()[booleanPreferencesKey("$profile/enabled")] ?: false
    suspend fun setEnabled(profile: String, enabled: Boolean) { store.edit { it[booleanPreferencesKey("$profile/enabled")] = enabled } }
    suspend fun muted(profile: String, conversation: String) = store.data.first()[booleanPreferencesKey("$profile/mute/$conversation")] ?: false
    suspend fun setMuted(profile: String, conversation: String, muted: Boolean) { store.edit { it[booleanPreferencesKey("$profile/mute/$conversation")] = muted } }
    suspend fun remove(profile: String) { store.edit { prefs -> prefs.asMap().keys.filter { it.name.startsWith("$profile/") }.forEach { prefs.remove(it) } } }
}

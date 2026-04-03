package org.xmtp.android.library

// Re-export all shared types under the old package name for backwards compatibility

// Client.kt
typealias PreEventCallback = org.xmtp.kotlin.PreEventCallback
typealias ProcessType = org.xmtp.kotlin.ProcessType
typealias MessageMetadata = org.xmtp.kotlin.MessageMetadata
typealias ClientOptions = org.xmtp.kotlin.ClientOptions
typealias ForkRecoveryPolicy = org.xmtp.kotlin.ForkRecoveryPolicy
typealias ForkRecoveryOptions = org.xmtp.kotlin.ForkRecoveryOptions
typealias InboxId = org.xmtp.kotlin.InboxId
typealias Client = org.xmtp.kotlin.Client

// CodecRegistry.kt
typealias CodecRegistry = org.xmtp.kotlin.CodecRegistry

// Conversation.kt
typealias Conversation = org.xmtp.kotlin.Conversation

// Conversations.kt
typealias GroupSyncSummary = org.xmtp.kotlin.GroupSyncSummary
typealias Conversations = org.xmtp.kotlin.Conversations

// Crypto.kt
typealias CipherText = org.xmtp.kotlin.CipherText
typealias Crypto = org.xmtp.kotlin.Crypto

// DelicateApi — annotation classes with @RequiresOptIn cannot be typealiased.
// Use org.xmtp.kotlin.DelicateApi directly.

// Dm.kt
typealias Dm = org.xmtp.kotlin.Dm

// EncodedContentCompression.kt
typealias EncodedContentCompression = org.xmtp.kotlin.EncodedContentCompression

// Group.kt
typealias Group = org.xmtp.kotlin.Group

// KeyUtil.kt
typealias KeyUtil = org.xmtp.kotlin.KeyUtil

// PrivatePreferences.kt
typealias ConsentState = org.xmtp.kotlin.ConsentState
typealias EntryType = org.xmtp.kotlin.EntryType
typealias PreferenceType = org.xmtp.kotlin.PreferenceType
typealias ConsentRecord = org.xmtp.kotlin.ConsentRecord
typealias PrivatePreferences = org.xmtp.kotlin.PrivatePreferences

// SendOptions.kt
typealias SendOptions = org.xmtp.kotlin.SendOptions
typealias MessageVisibilityOptions = org.xmtp.kotlin.MessageVisibilityOptions

// SignedData.kt
typealias SignedData = org.xmtp.kotlin.SignedData

// SigningKey.kt
typealias SigningKey = org.xmtp.kotlin.SigningKey
typealias SignerType = org.xmtp.kotlin.SignerType

// Topic.kt
typealias Topic = org.xmtp.kotlin.Topic

// Util.kt
typealias Util = org.xmtp.kotlin.Util

// XMTPDebugInformation.kt
typealias XMTPDebugInformation = org.xmtp.kotlin.XMTPDebugInformation
typealias ApiStats = org.xmtp.kotlin.ApiStats
typealias IdentityStats = org.xmtp.kotlin.IdentityStats

// XMTPEnvironment.kt
typealias XMTPEnvironment = org.xmtp.kotlin.XMTPEnvironment

// XMTPException.kt
typealias XMTPException = org.xmtp.kotlin.XMTPException

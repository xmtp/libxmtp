package org.xmtp.android.library

sealed class Topic {
    @Suppress("ktlint:standard:class-naming")
    data class userWelcome(
        val installationId: String?,
    ) : Topic()

    @Suppress("ktlint:standard:class-naming")
    data class groupMessage(
        val groupId: String?,
    ) : Topic()

    val description: String
        get() {
            return when (this) {
                is groupMessage -> wrapMls("g-$groupId")
                is userWelcome -> wrapMls("w-$installationId")
            }
        }

    private fun wrapMls(value: String): String = "/xmtp/mls/1/$value/proto"
}

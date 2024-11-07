package org.xmtp.android.library.messages

sealed class Topic {
    data class userWelcome(val installationId: String?) : Topic()
    data class groupMessage(val groupId: String?) : Topic()

    val description: String
        get() {
            return when (this) {
                is groupMessage -> wrapMls("g-$groupId")
                is userWelcome -> wrapMls("w-$installationId")
            }
        }

    private fun wrapMls(value: String): String = "/xmtp/mls/1/$value/proto"
}

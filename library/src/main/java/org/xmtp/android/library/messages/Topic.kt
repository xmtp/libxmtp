package org.xmtp.android.library.messages

sealed class Topic {
    data class userPrivateStoreKeyBundle(val v1: String) : Topic()
    data class contact(val v1: String) : Topic()
    data class userIntro(val v1: String) : Topic()
    data class userInvite(val v1: String) : Topic()
    data class directMessageV1(val v1: String, val v2: String) : Topic()
    data class directMessageV2(val v2: String) : Topic()

    val description: String
        get() {
            return when (this) {
                is userPrivateStoreKeyBundle -> wrap("privatestore-$v1")
                is contact -> wrap("contact-$v1")
                is userIntro -> wrap("intro-$v1")
                is userInvite -> wrap("invite-$v1")
                is directMessageV1 -> {
                    val addresses = listOf(v1, v2).sorted().joinToString(separator = "-")
                    wrap("dm-$addresses")
                }
                is directMessageV2 -> wrap("m-$v2")
            }
        }

    private fun wrap(value: String): String = "/xmtp/0/$value/proto"
}

package org.xmtp.android.library.messages

sealed class Topic {
    data class userPrivateStoreKeyBundle(val address: String?) : Topic()
    data class contact(val address: String?) : Topic()
    data class userIntro(val address: String?) : Topic()
    data class userInvite(val address: String?) : Topic()
    data class directMessageV1(val address1: String?, val address2: String?) : Topic()
    data class directMessageV2(val addresses: String?) : Topic()
    data class preferenceList(val identifier: String?) : Topic()

    val description: String
        get() {
            return when (this) {
                is userPrivateStoreKeyBundle -> wrap("privatestore-$address/key_bundle")
                is contact -> wrap("contact-$address")
                is userIntro -> wrap("intro-$address")
                is userInvite -> wrap("invite-$address")
                is directMessageV1 -> {
                    val addresses = arrayOf(address1, address2)
                    addresses.sort()
                    wrap("dm-${addresses.joinToString(separator = "-")}")
                }

                is directMessageV2 -> wrap("m-$addresses")
                is preferenceList -> wrap("userpreferences-$identifier")
            }
        }

    private fun wrap(value: String): String = "/xmtp/0/$value/proto"

    companion object {
        fun isValidTopic(topic: String): Boolean {
            val regex = Regex("^[\\x00-\\x7F]+$") // Use this regex to filter non ASCII chars
            val index = topic.indexOf("0/")
            if (index != -1) {
                val unwrappedTopic = topic.substring(index + 2, topic.lastIndexOf("/proto"))
                return unwrappedTopic.matches(regex)
            }
            return false
        }
    }
}

package uniffi.xmtp_dh.org.xmtp.android.library

import org.xmtp.android.library.Client
import uniffi.xmtp_dh.org.xmtp.android.library.codecs.GroupChatMemberAddedCodec
import uniffi.xmtp_dh.org.xmtp.android.library.codecs.GroupChatTitleChangedCodec

class GroupChat {
    companion object {
        fun registerCodecs() {
            Client.register(codec = GroupChatMemberAddedCodec())
            Client.register(codec = GroupChatTitleChangedCodec())
        }
    }
}

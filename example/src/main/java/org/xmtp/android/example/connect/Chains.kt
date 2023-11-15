package org.xmtp.android.example.connect

import androidx.annotation.DrawableRes
import org.xmtp.android.example.R


fun getPersonalSignBody(message:String, account: String): String {
    val msg = message.encodeToByteArray()
        .joinToString(separator = "", prefix = "0x") { eachByte -> "%02x".format(eachByte) }
    return "[\"$msg\", \"$account\"]"
}
enum class Chains(
    val chainName: String,
    val chainNamespace: String,
    val chainReference: String,
    @DrawableRes val icon: Int,
    val methods: List<String>,
    val events: List<String>,
) {
    ETHEREUM_MAIN(
        chainName = "Ethereum",
        chainNamespace = Info.Eth.chain,
        chainReference = "1",
        icon = R.drawable.ic_ethereum,
        methods = Info.Eth.defaultMethods,
        events = Info.Eth.defaultEvents,
    )
}

sealed class Info {
    abstract val chain: String
    abstract val defaultEvents: List<String>
    abstract val defaultMethods: List<String>

    object Eth : Info() {
        override val chain = "eip155"
        override val defaultEvents: List<String> = listOf("chainChanged", "accountsChanged")
        override val defaultMethods: List<String> = listOf(
            "eth_sendTransaction",
            "personal_sign",
            "eth_sign",
            "eth_signTypedData"
        )
    }

}
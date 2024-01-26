package org.xmtp.android.example.account

import android.net.Uri
import com.walletconnect.wcmodal.client.Modal
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.web3j.crypto.Keys
import org.xmtp.android.example.connect.getPersonalSignBody
import org.xmtp.android.example.extension.requestMethod
import org.xmtp.android.library.SigningKey
import org.xmtp.android.library.messages.SignatureBuilder
import org.xmtp.proto.message.contents.SignatureOuterClass


data class WalletConnectV2Account(
    val session: Modal.Model.ApprovedSession,
    val chain: String,
    private val sendSessionRequestDeepLink: (Uri) -> Unit,
) :
    SigningKey {
    override val address: String
        get() = Keys.toChecksumAddress(
            session.namespaces.getValue(chain).accounts[0].substringAfterLast(
                ":"
            )
        )

    override suspend fun sign(data: ByteArray): SignatureOuterClass.Signature? {
        return signLegacy(String(data))
    }

    override fun sign(text: String): ByteArray {
        val (parentChain, chainId, account) = session.namespaces.getValue(chain).accounts[0].split(":")
        val requestParams = session.namespaces.getValue(chain).methods.find { method ->
            method == "personal_sign"
        }?.let { method ->
            Modal.Params.Request(
                sessionTopic = session.topic,
                method = method,
                params = getPersonalSignBody(text, account),
                chainId = "$parentChain:$chainId"
            )
        }

        runCatching {
            runBlocking {
                requestMethod(requestParams!!, sendSessionRequestDeepLink).first().getOrThrow()
            }
        }.onSuccess {
            return it
        }.onFailure {}

        return byteArrayOf()
    }

    override suspend fun signLegacy(message: String): SignatureOuterClass.Signature? {
        val (parentChain, chainId, account) = session.namespaces.getValue(chain).accounts[0].split(":")
        val requestParams = session.namespaces.getValue(chain).methods.find { method ->
            method == "personal_sign"
        }?.let { method ->
            Modal.Params.Request(
                sessionTopic = session.topic,
                method = method,
                params = getPersonalSignBody(message, account),
                chainId = "$parentChain:$chainId"
            )
        }
        runCatching {
            requestMethod(requestParams!!, sendSessionRequestDeepLink).first().getOrThrow()
        }
            .onSuccess {
                return SignatureBuilder.buildFromSignatureData(it)
            }
            .onFailure {}

        return null
    }
}

package org.xmtp.android.example.extension

import android.net.Uri
import androidx.core.net.toUri
import com.walletconnect.wcmodal.client.Modal
import com.walletconnect.wcmodal.client.WalletConnectModal
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.channels.trySendBlocking
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.withContext
import org.web3j.utils.Numeric
import org.xmtp.android.example.connect.DappDelegate
import timber.log.Timber

suspend fun requestMethod(
    requestParams: Modal.Params.Request,
    sendSessionRequestDeepLink: (Uri) -> Unit
): Flow<Result<ByteArray>> {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    return withContext(Dispatchers.IO) {
        callbackFlow {
            WalletConnectModal.request(
                request = requestParams,
                onSuccess = { sentRequest ->
                    WalletConnectModal.getActiveSessionByTopic(requestParams.sessionTopic)?.redirect?.toUri()
                        ?.let { deepLinkUri ->
                            sendSessionRequestDeepLink(deepLinkUri)
                        }
                    onResponse(scope, this, sentRequest)

                },
                onError = { Timber.e(it.throwable) }
            )
            awaitClose {  }
        }

    }

}

private fun onResponse(
    scope: CoroutineScope,
    continuation: ProducerScope<Result<ByteArray>>,
    sentRequest: Modal.Model.SentRequest
) {
    DappDelegate.wcEventModels
        .filterNotNull()
        .onEach { event ->
            when (event) {
                is Modal.Model.SessionRequestResponse -> {
                    if (event.topic == sentRequest.sessionTopic && event.result.id == sentRequest.requestId) {
                        when (val res = event.result) {
                            is Modal.Model.JsonRpcResponse.JsonRpcResult -> {
                                var result = res.result
                                if (result.startsWith("0x") && result.length == 132) {
                                    result = result.drop(2)
                                }

                                val resultData = Numeric.hexStringToByteArray(result)

                                // Ensure we have a valid recovery byte
                                resultData[resultData.size - 1] =
                                    (1 - resultData[resultData.size - 1] % 2).toByte()

                                continuation.trySendBlocking(Result.success(resultData))
                            }
                            is Modal.Model.JsonRpcResponse.JsonRpcError -> {
                                continuation.trySendBlocking(Result.failure(Throwable(res.message)))
                            }
                        }
                    } else continuation.trySendBlocking(Result.failure(Throwable("The result id is different from the request id!")))
                }

                else -> {}
            }
        }.launchIn(scope)

}
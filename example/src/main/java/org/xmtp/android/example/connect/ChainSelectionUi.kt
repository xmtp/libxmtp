package org.xmtp.android.example.connect


data class ChainSelectionUi(
    val chainName: String,
    val chainNamespace: String,
    val chainReference: String,
    val icon: Int,
    val methods: List<String>,
    val events: List<String>,
    var isSelected: Boolean = false,
) {
    val chainId = "${chainNamespace}:${chainReference}"
}

fun Chains.toChainUiState() = ChainSelectionUi(chainName, chainNamespace, chainReference, icon, methods, events)

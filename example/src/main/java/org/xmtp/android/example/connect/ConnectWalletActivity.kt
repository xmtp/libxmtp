package org.xmtp.android.example.connect

import android.accounts.Account
import android.accounts.AccountManager
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.view.View
import android.widget.Toast
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.isVisible
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch
import org.xmtp.android.example.MainActivity
import org.xmtp.android.example.R
import org.xmtp.android.example.databinding.ActivityConnectWalletBinding

class ConnectWalletActivity : AppCompatActivity() {

    companion object {
        private const val WC_URI_SCHEME = "wc://wc?uri="
    }

    private val viewModel: ConnectWalletViewModel by viewModels()
    private lateinit var binding: ActivityConnectWalletBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        binding = ActivityConnectWalletBinding.inflate(layoutInflater)
        setContentView(binding.root)

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.uiState.collect(::ensureUiState)
            }
        }

        binding.generateButton.setOnClickListener {
            viewModel.generateWallet()
        }

        val isConnectWalletAvailable = isConnectAvailable()
        binding.connectButton.isEnabled = isConnectWalletAvailable
        binding.connectError.isVisible = !isConnectWalletAvailable
        binding.connectButton.setOnClickListener {
            binding.connectButton.start(viewModel.walletConnectKit, ::onConnected, ::onDisconnected)
        }
    }

    private fun onConnected(address: String) {
        viewModel.connectWallet()
    }

    private fun onDisconnected() {
        // No-op currently.
    }

    private fun isConnectAvailable(): Boolean {
        val wcIntent = Intent(Intent.ACTION_VIEW).apply {
            data = Uri.parse(WC_URI_SCHEME)
        }
        return wcIntent.resolveActivity(packageManager) != null
    }

    private fun ensureUiState(uiState: ConnectWalletViewModel.ConnectUiState) {
        when (uiState) {
            is ConnectWalletViewModel.ConnectUiState.Error -> showError(uiState.message)
            ConnectWalletViewModel.ConnectUiState.Loading -> showLoading()
            is ConnectWalletViewModel.ConnectUiState.Success -> signIn(
                uiState.address,
                uiState.encodedKeyData
            )
            ConnectWalletViewModel.ConnectUiState.Unknown -> Unit
        }
    }

    private fun signIn(address: String, encodedKey: String) {
        val accountManager = AccountManager.get(this)
        Account(address, resources.getString(R.string.account_type)).also { account ->
            accountManager.addAccountExplicitly(account, encodedKey, null)
        }
        startActivity(Intent(this, MainActivity::class.java))
        finish()
    }

    private fun showError(message: String) {
        binding.progress.visibility = View.GONE
        binding.generateButton.visibility = View.VISIBLE
        binding.connectButton.visibility = View.VISIBLE
        binding.connectError.isVisible = !isConnectAvailable()
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
    }

    private fun showLoading() {
        binding.progress.visibility = View.VISIBLE
        binding.generateButton.visibility = View.GONE
        binding.connectButton.visibility = View.GONE
        binding.connectError.visibility = View.GONE
    }
}

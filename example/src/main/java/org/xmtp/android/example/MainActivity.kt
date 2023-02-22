package org.xmtp.android.example

import android.accounts.AccountManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.widget.Toast
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import kotlinx.coroutines.launch
import org.xmtp.android.example.connect.ConnectWalletActivity
import org.xmtp.android.example.conversation.ConversationFooterViewHolder
import org.xmtp.android.example.conversation.ConversationsAdapter
import org.xmtp.android.example.databinding.ActivityMainBinding

class MainActivity : AppCompatActivity(),
    ConversationFooterViewHolder.OnConversationFooterClickListener {

    private val viewModel: MainViewModel by viewModels()
    private lateinit var binding: ActivityMainBinding
    private lateinit var accountManager: AccountManager
    private lateinit var adapter: ConversationsAdapter

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        accountManager = AccountManager.get(this)

        val keys = loadKeys()
        if (keys == null) {
            showSignIn()
            return
        }

        viewModel.createClient(keys)

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
        setSupportActionBar(binding.toolbar)

        adapter = ConversationsAdapter(footerClickListener = this)
        binding.list.layoutManager = LinearLayoutManager(this)
        binding.list.adapter = adapter
        binding.refresh.setOnRefreshListener {
            viewModel.fetchConversations()
        }

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.clientState.collect(::ensureClientState)
            }
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.uiState.collect(::ensureUiState)
            }
        }
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_main, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        return when (item.itemId) {
            R.id.disconnect -> {
                disconnectWallet()
                true
            }
            R.id.copy_address -> {
                copyWalletAddress()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }

    override fun onFooterClick(address: String) {
        copyWalletAddress()
    }

    private fun ensureClientState(clientState: MainViewModel.ClientState) {
        when (clientState) {
            is MainViewModel.ClientState.Ready -> viewModel.fetchConversations()
            is MainViewModel.ClientState.Error -> showError(clientState.message)
            is MainViewModel.ClientState.Unknown -> Unit
        }
    }

    private fun ensureUiState(uiState: MainViewModel.UiState) {
        binding.progress.visibility = View.GONE
        when (uiState) {
            is MainViewModel.UiState.Loading -> {
                if (uiState.listItems.isNullOrEmpty()) {
                    binding.progress.visibility = View.VISIBLE
                } else {
                    adapter.setData(uiState.listItems)
                }
            }
            is MainViewModel.UiState.Success -> {
                binding.refresh.isRefreshing = false
                adapter.setData(uiState.listItems)
            }
            is MainViewModel.UiState.Error -> {
                binding.refresh.isRefreshing = false
                showError(uiState.message)
            }
        }
    }

    private fun loadKeys(): String? {
        val accounts = accountManager.getAccountsByType(resources.getString(R.string.account_type))
        val account = accounts.firstOrNull() ?: return null
        return accountManager.getPassword(account)
    }

    private fun showSignIn() {
        startActivity(Intent(this, ConnectWalletActivity::class.java))
        finish()
    }

    private fun showError(message: String) {
        val error = message.ifBlank { resources.getString(R.string.error) }
        Toast.makeText(this, error, Toast.LENGTH_SHORT).show()
    }

    private fun disconnectWallet() {
        viewModel.clearClient()
        val accounts = accountManager.getAccountsByType(resources.getString(R.string.account_type))
        accounts.forEach { account ->
            accountManager.removeAccount(account, null, null, null)
        }
        showSignIn()
    }

    private fun copyWalletAddress() {
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val clip = ClipData.newPlainText("address", viewModel.client?.address.orEmpty())
        clipboard.setPrimaryClip(clip)
    }
}

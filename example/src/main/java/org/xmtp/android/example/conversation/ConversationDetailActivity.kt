package org.xmtp.android.example.conversation

import android.R.id.home
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
import androidx.core.widget.addTextChangedListener
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import kotlinx.coroutines.launch
import org.xmtp.android.example.R
import org.xmtp.android.example.databinding.ActivityConversationDetailBinding
import org.xmtp.android.example.extension.truncatedAddress
import org.xmtp.android.example.message.MessageAdapter

class ConversationDetailActivity : AppCompatActivity() {

    private lateinit var binding: ActivityConversationDetailBinding
    private lateinit var adapter: MessageAdapter

    private val viewModel: ConversationDetailViewModel by viewModels()

    private val peerAddress
        get() = intent.extras?.getString(EXTRA_PEER_ADDRESS)

    companion object {
        const val EXTRA_CONVERSATION_TOPIC = "EXTRA_CONVERSATION_TOPIC"
        private const val EXTRA_PEER_ADDRESS = "EXTRA_PEER_ADDRESS"

        fun intent(context: Context, topic: String, peerAddress: String): Intent {
            return Intent(context, ConversationDetailActivity::class.java).apply {
                putExtra(EXTRA_CONVERSATION_TOPIC, topic)
                putExtra(EXTRA_PEER_ADDRESS, peerAddress)
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        viewModel.setConversationTopic(intent.extras?.getString(EXTRA_CONVERSATION_TOPIC))

        binding = ActivityConversationDetailBinding.inflate(layoutInflater)
        setContentView(binding.root)
        setSupportActionBar(binding.toolbar)
        supportActionBar?.setDisplayHomeAsUpEnabled(true)
        supportActionBar?.subtitle = peerAddress?.truncatedAddress()

        adapter = MessageAdapter()
        binding.list.layoutManager =
            LinearLayoutManager(this, RecyclerView.VERTICAL, true)
        binding.list.adapter = adapter

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.uiState.collect(::ensureUiState)
            }
        }

        binding.messageEditText.requestFocus()
        binding.messageEditText.addTextChangedListener {
            val sendEnabled = !binding.messageEditText.text.isNullOrBlank()
            binding.sendButton.isEnabled = sendEnabled
        }

        binding.sendButton.setOnClickListener {
            val flow = viewModel.sendMessage(binding.messageEditText.text.toString())
            lifecycleScope.launch {
                repeatOnLifecycle(Lifecycle.State.STARTED) {
                    flow.collect(::ensureSendState)
                }
            }
        }

        lifecycleScope.launch {
            this@ConversationDetailActivity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.streamMessages.collect {
                    if (it.isNotEmpty()) {
                        adapter.addItem(it.first())
                    }
                }
            }
        }

        binding.refresh.setOnRefreshListener {
            viewModel.fetchMessages()
        }

        viewModel.fetchMessages()
    }

    override fun onCreateOptionsMenu(menu: Menu): Boolean {
        menuInflater.inflate(R.menu.menu_main, menu)
        return true
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        return when (item.itemId) {
            home -> {
                finish()
                true
            }
            R.id.copy_address -> {
                copyWalletAddress()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }

    private fun copyWalletAddress() {
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val clip = ClipData.newPlainText("peer_address", peerAddress)
        clipboard.setPrimaryClip(clip)
    }

    private fun ensureUiState(uiState: ConversationDetailViewModel.UiState) {
        binding.progress.visibility = View.GONE
        when (uiState) {
            is ConversationDetailViewModel.UiState.Loading -> {
                if (uiState.listItems.isNullOrEmpty()) {
                    binding.progress.visibility = View.VISIBLE
                } else {
                    adapter.setData(uiState.listItems)
                }
            }
            is ConversationDetailViewModel.UiState.Success -> {
                binding.refresh.isRefreshing = false
                adapter.setData(uiState.listItems)
            }
            is ConversationDetailViewModel.UiState.Error -> {
                binding.refresh.isRefreshing = false
                showError(uiState.message)
            }
        }
    }

    private fun ensureSendState(sendState: ConversationDetailViewModel.SendMessageState) {
        when (sendState) {
            is ConversationDetailViewModel.SendMessageState.Error -> {
                showError(sendState.message)
            }
            ConversationDetailViewModel.SendMessageState.Loading -> {
                binding.sendButton.isEnabled = false
                binding.messageEditText.isEnabled = false
            }
            ConversationDetailViewModel.SendMessageState.Success -> {
                binding.messageEditText.text.clear()
                binding.messageEditText.isEnabled = true
                binding.sendButton.isEnabled = true
                viewModel.fetchMessages()
            }
        }
    }

    private fun showError(message: String) {
        val error = message.ifBlank { resources.getString(R.string.error) }
        Toast.makeText(this, error, Toast.LENGTH_SHORT).show()
    }
}

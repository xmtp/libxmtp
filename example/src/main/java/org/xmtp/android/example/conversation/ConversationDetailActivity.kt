package org.xmtp.android.example.conversation

import android.R.id.home
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.view.MenuItem
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import org.xmtp.android.example.databinding.ActivityConversationDetailBinding

class ConversationDetailActivity : AppCompatActivity() {

    private lateinit var binding: ActivityConversationDetailBinding

    private val viewModel: ConversationDetailViewModel by viewModels()

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
        supportActionBar?.subtitle = intent.extras?.getString(EXTRA_PEER_ADDRESS)

        viewModel.fetchMessages()
    }

    override fun onOptionsItemSelected(item: MenuItem): Boolean {
        when (item.itemId) {
            home -> {
                finish()
                return true
            }
        }
        return super.onOptionsItemSelected(item)
    }
}

package org.xmtp.android.example

import android.Manifest
import android.accounts.AccountManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.util.Log
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.widget.Toast
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import org.xmtp.android.example.connect.ConnectWalletActivity
import org.xmtp.android.example.conversation.ConversationDetailActivity
import org.xmtp.android.example.conversation.ConversationsAdapter
import org.xmtp.android.example.conversation.ConversationsClickListener
import org.xmtp.android.example.conversation.NewConversationBottomSheet
import org.xmtp.android.example.conversation.NewGroupBottomSheet
import org.xmtp.android.example.databinding.ActivityMainBinding
import org.xmtp.android.example.pushnotifications.PushNotificationTokenManager
import org.xmtp.android.example.utils.KeyUtil
import org.xmtp.android.library.Client
import org.xmtp.android.library.Conversation
import org.xmtp.android.example.logs.LogViewerBottomSheet
import uniffi.xmtpv3.FfiLogLevel
import uniffi.xmtpv3.FfiLogRotation


class MainActivity : AppCompatActivity(),
    ConversationsClickListener {

    private val viewModel: MainViewModel by viewModels()
    private lateinit var binding: ActivityMainBinding
    private lateinit var accountManager: AccountManager
    private lateinit var adapter: ConversationsAdapter
    private var bottomSheet: NewConversationBottomSheet? = null
    private var groupBottomSheet: NewGroupBottomSheet? = null
    private var logsBottomSheet: LogViewerBottomSheet? = null
    private val REQUEST_CODE_POST_NOTIFICATIONS = 101
    
    // Add constant for SharedPreferences
    companion object {
        private const val PREFS_NAME = "XMTPPreferences"
        private const val KEY_LOGS_ACTIVATED = "logs_activated"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        accountManager = AccountManager.get(this)
        checkAndRequestPermissions()
        PushNotificationTokenManager.init(this, "10.0.2.2:8080")
        viewModel.setupPush()

        val keys = KeyUtil(this).loadKeys()
        if (keys == null) {
            showSignIn()
            return
        }

        ClientManager.createClient(keys, this)

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
        setSupportActionBar(binding.toolbar)

        adapter = ConversationsAdapter(clickListener = this)
        binding.list.layoutManager = LinearLayoutManager(this)
        binding.list.adapter = adapter
        binding.refresh.setOnRefreshListener {
            if (ClientManager.clientState.value is ClientManager.ClientState.Ready) {
                viewModel.fetchConversations()
            }
        }

        binding.fab.setOnClickListener {
            openConversationDetail()
        }

        binding.groupFab.setOnClickListener {
            openGroupDetail()
        }

        binding.logsFab.setOnClickListener {
            openLogsViewer()
        }

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                ClientManager.clientState.collect(::ensureClientState)
            }
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.uiState.collect(::ensureUiState)
            }
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                viewModel.stream.collect(::addStreamedItem)
            }
        }

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                ClientManager.clientState.collect { state ->
                    when (state) {
                        is ClientManager.ClientState.Error -> {
                            retryCreateClientWithBackoff()
                        }
                        is ClientManager.ClientState.Ready -> {
                            retryJob?.cancel()
                            retryJob = null
                        }
                        else -> Unit
                    }
                }
            }
        }
    }

    private var retryJob: Job? = null

    private fun retryCreateClientWithBackoff() {
        if (retryJob?.isActive == true) return // Prevent duplicate retries

        val keys = KeyUtil(this).loadKeys() ?: run {
            showSignIn()
            return
        }

        retryJob = lifecycleScope.launch(Dispatchers.IO) {
            var delayTime = 1000L // start at 1 second
            val maxDelay = 30000L // cap at 30 seconds

            while (ClientManager.clientState.value is ClientManager.ClientState.Error && isActive) {
                Log.d("RETRY", "Retrying client creation after ${delayTime}ms")
                ClientManager.createClient(keys, this@MainActivity)

                delay(delayTime)
                delayTime = (delayTime * 2).coerceAtMost(maxDelay) // exponential backoff
            }

            Log.d("RETRY", "Retry stopped: current state=${ClientManager.clientState.value}")
        }
    }

    override fun onResume() {
        super.onResume()
        // Check if logs were previously activated and reactivate if needed
        if (isLogsActivated()) {
            Client.activatePersistentLibXMTPLogWriter(applicationContext, FfiLogLevel.DEBUG, FfiLogRotation.MINUTELY, 3)
        }

        // If we're still in error state, make sure retry is running
        if (ClientManager.clientState.value is ClientManager.ClientState.Error) {
            val keys = KeyUtil(this).loadKeys()
            if (keys == null) {
                showSignIn()
                return
            }
            retryCreateClientWithBackoff()
        }

    }

    override fun onDestroy() {
        bottomSheet?.dismiss()
        groupBottomSheet?.dismiss()
        logsBottomSheet?.dismiss()
        super.onDestroy()
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
            R.id.activate_logs -> {
                Client.activatePersistentLibXMTPLogWriter(applicationContext, FfiLogLevel.DEBUG, FfiLogRotation.MINUTELY, 3)
                setLogsActivated(true)
                Toast.makeText(this, "Persistent logs activated", Toast.LENGTH_SHORT).show()
                true
            }
            R.id.deactivate_logs -> {
                Client.deactivatePersistentLibXMTPLogWriter()
                setLogsActivated(false)
                Toast.makeText(this, "Persistent logs deactivated", Toast.LENGTH_SHORT).show()
                true
            }
            else -> super.onOptionsItemSelected(item)
        }
    }

    override fun onConversationClick(conversation: Conversation) {
        startActivity(
            ConversationDetailActivity.intent(
                this,
                topic = conversation.topic,
                peerAddress = conversation.id
            )
        )
    }

    override fun onFooterClick(address: String) {
        copyWalletAddress()
    }

    private fun ensureClientState(clientState: ClientManager.ClientState) {
        when (clientState) {
            is ClientManager.ClientState.Ready -> {
                viewModel.fetchConversations()
                binding.fab.visibility = View.VISIBLE
                binding.groupFab.visibility = View.VISIBLE
                binding.logsFab.visibility = View.VISIBLE
            }
            is ClientManager.ClientState.Error -> showError(clientState.message)
            is ClientManager.ClientState.Unknown -> Unit
        }
    }

    private fun addStreamedItem(item: MainViewModel.MainListItem?) {
        item?.let {
            adapter.addItem(item)
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

    private fun showSignIn() {
        startActivity(Intent(this, ConnectWalletActivity::class.java))
        finish()
    }

    private fun showError(message: String) {
        val error = message.ifBlank { resources.getString(R.string.error) }
        Log.e("MainActivity", message)
        Toast.makeText(this, error, Toast.LENGTH_SHORT).show()
    }

    private fun disconnectWallet() {
        ClientManager.clearClient()
        PushNotificationTokenManager.clearXMTPPush()
        val accounts = accountManager.getAccountsByType(resources.getString(R.string.account_type))
        accounts.forEach { account ->
            accountManager.removeAccount(account, null, null, null)
        }
        showSignIn()
    }

    private fun copyWalletAddress() {
        val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val clip = ClipData.newPlainText("inboxId", ClientManager.client.inboxId)
        clipboard.setPrimaryClip(clip)
    }

    private fun openConversationDetail() {
        bottomSheet = NewConversationBottomSheet.newInstance()
        bottomSheet?.show(
            supportFragmentManager,
            NewConversationBottomSheet.TAG
        )
    }
    private fun openGroupDetail() {
        groupBottomSheet = NewGroupBottomSheet.newInstance()
        groupBottomSheet?.show(
            supportFragmentManager,
            NewGroupBottomSheet.TAG
        )
    }

    private fun openLogsViewer() {
        logsBottomSheet = LogViewerBottomSheet.newInstance()
        logsBottomSheet?.show(
            supportFragmentManager,
            LogViewerBottomSheet.TAG
        )
    }

    private fun checkAndRequestPermissions() {
        if (ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.POST_NOTIFICATIONS
            ) != PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(
                this,
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                REQUEST_CODE_POST_NOTIFICATIONS
            )
        }
    }

    // Add helper methods to manage log activation state
    private fun isLogsActivated(): Boolean {
        val prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        return prefs.getBoolean(KEY_LOGS_ACTIVATED, false)
    }
    
    private fun setLogsActivated(activated: Boolean) {
        val prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        prefs.edit().putBoolean(KEY_LOGS_ACTIVATED, activated).apply()
    }
}

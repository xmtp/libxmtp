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

    private lateinit var binding: ActivityConnectWalletBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        binding = ActivityConnectWalletBinding.inflate(layoutInflater)
        setContentView(binding.root)




    }


}

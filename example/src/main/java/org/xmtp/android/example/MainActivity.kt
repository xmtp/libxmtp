package org.xmtp.android.example

import android.content.Intent
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import org.xmtp.android.example.connect.ConnectWalletActivity
import org.xmtp.android.example.databinding.ActivityMainBinding

class MainActivity : AppCompatActivity() {

    private lateinit var binding: ActivityMainBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (!ensureAuthenticated()) {
            return
        }

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        binding.address.text = UserPreferences.signedInAddress(this)
        binding.disconnect.setOnClickListener {
            disconnectWallet()
        }
    }

    private fun ensureAuthenticated(): Boolean {
        if (UserPreferences.signedInAddress(this).isNullOrEmpty()) {
            startActivity(Intent(this, ConnectWalletActivity::class.java))
            finish()
            return false
        }
        return true
    }

    private fun disconnectWallet() {
        UserPreferences.setSignedInAddress(this, null)
        ensureAuthenticated()
    }
}

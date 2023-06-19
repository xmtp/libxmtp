package com.example.xmtpv3_example

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.widget.TextView
import com.example.xmtpv3_example.R.id.selftest_output
import kotlinx.coroutines.runBlocking

const val EMULATOR_LOCALHOST_ADDRESS = "http://10.0.2.2:5556"

// An example Android app testing the end-to-end flow through Rust
// Run setup_android_example.sh to set it up
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        runBlocking {
            super.onCreate(savedInstanceState)
            setContentView(R.layout.activity_main)

            val textView: TextView = findViewById<TextView>(selftest_output)
            try {
                val client = uniffi.xmtpv3.createClient(EMULATOR_LOCALHOST_ADDRESS, false);
                textView.text = "Client constructed, wallet address: " + client.walletAddress();
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message;
            }
        }
    }

}
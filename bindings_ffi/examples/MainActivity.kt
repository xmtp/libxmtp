package com.example.xmtpv3_example

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.widget.TextView
import com.example.xmtpv3_example.R.id.selftest_output
import kotlinx.coroutines.runBlocking

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        runBlocking {
            super.onCreate(savedInstanceState)
            setContentView(R.layout.activity_main)

            val textView: TextView = findViewById<TextView>(selftest_output)
            try {
                val client = uniffi.xmtpv3.createClient("http://localhost:5555", false);
                textView.text = "Client constructed, wallet address: " // + client.walletAddress();
            } catch (e: Exception) {
                textView.text = "Failed to construct client: " + e.message;
            }
        }
    }

}
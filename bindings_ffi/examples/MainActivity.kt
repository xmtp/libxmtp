package com.example.xmtpv3_example

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.widget.TextView
import com.example.xmtpv3_example.R.id.selftest_output

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val textView: TextView = findViewById<TextView>(selftest_output)
        if (uniffi.xmtpv3.add(1u, 2u) == 3u) {
            textView.text = "Test passed"
        } else {
            textView.text = "Test failed"
        }
    }

}
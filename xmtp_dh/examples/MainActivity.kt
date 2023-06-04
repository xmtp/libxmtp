package com.example.xmtpv3_example

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.widget.TextView
import com.example.xmtpv3_example.R.id.selftest_output

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

//        uniffi.xmtp_dh.sl
        val privateA = mutableListOf<UByte>(
            107u,  41u, 134u,  89u, 51u, 186u, 228u,  48u,
            87u,  94u,  90u,  47u, 46u,  77u, 210u,  51u,
            203u, 234u,  31u, 131u,  7u, 237u, 134u,  20u,
            107u, 241u, 244u,   2u, 98u, 224u, 187u, 211u
        )
        val publicB = mutableListOf<UByte>(
            4u,  68u, 131u,  28u, 110u, 176u, 218u,  55u, 196u, 214u,  40u,
            193u, 194u, 149u, 163u, 153u,  28u,  78u, 229u, 231u, 137u, 155u,
            18u, 159u, 162u, 180u, 133u,  78u,  77u,  79u,  57u, 232u, 133u,
            209u, 100u,  47u,  32u,  17u, 221u,  53u, 251u,  43u, 246u, 199u,
            200u,  16u,  74u,  49u,  21u, 248u,  90u, 135u, 162u,  63u, 195u,
            75u, 228u, 188u, 238u,  94u,  72u,  18u, 173u,  71u, 139u
        )
        // Generated using noble/secp256k1
        val expectedSecret = listOf<UByte>(
            4u, 122u,  59u,  94u, 250u, 158u, 116u,  59u, 129u,  56u,   6u,
            119u, 252u,  80u, 246u, 179u, 132u, 196u, 241u, 218u, 148u, 232u,
            52u, 184u, 107u, 186u, 121u, 197u,  54u,  70u, 161u, 204u, 167u,
            85u,  53u, 209u, 251u, 104u, 241u, 155u,  67u, 102u, 173u, 208u,
            25u,  73u, 127u, 209u,  83u,  69u,  26u, 238u,  79u, 185u, 219u,
            43u,   5u, 101u,  10u, 115u, 186u,  76u, 169u,  86u, 255u
        )
        val actualSecret: List<UByte> = uniffi.xmtp_dh.diffieHellmanK256(privateA, publicB)

        val textView: TextView = findViewById<TextView>(selftest_output)
        if (!expectedSecret.equals(actualSecret)) {
            textView.text = "Test 1 failed, didn't generate correct secret"
        }

        var test2Passed = false;
        try {
            publicB[0] = 1u;
            uniffi.xmtp_dh.diffieHellmanK256(privateA, publicB)
        } catch (e: uniffi.xmtp_dh.DiffieHellmanException) {
            test2Passed = true;
            textView.text = "All tests passed! Sample error message: " + e.message
        }

        if (!test2Passed) {
            textView.text = "Error handling test failed, didn't throw an exception"
        }
    }

}
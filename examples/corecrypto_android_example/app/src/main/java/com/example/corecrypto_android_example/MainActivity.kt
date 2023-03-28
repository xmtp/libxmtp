package com.example.corecrypto_android_example

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.widget.TextView
import com.example.corecrypto_android_example.R.id.android_text

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val textView: TextView = findViewById<TextView>(android_text)


        var hkdf_salt : List<UByte> = listOf(139u, 45u, 107u, 41u, 87u, 173u, 15u, 163u, 250u, 14u, 194u, 152u, 200u, 180u, 226u, 48u, 140u, 198u, 1u, 93u, 80u, 253u, 64u, 244u, 41u, 69u, 15u, 139u, 197u, 77u, 189u, 53u)
        var gcm_nonce : List<UByte> = listOf(55u, 245u, 104u, 8u, 28u, 107u, 41u, 76u, 54u, 166u, 179u, 183u)
        var payload : List<UByte> = listOf(29u, 166u, 18u, 126u, 14u, 51u, 186u, 211u, 216u, 75u, 24u, 3u, 137u, 77u, 83u, 46u, 162u, 125u, 138u, 179u, 183u, 125u, 96u, 93u, 70u, 57u, 95u, 207u, 85u, 199u, 180u, 152u, 5u, 238u, 57u, 184u, 250u, 185u, 32u, 126u, 50u, 79u, 154u, 92u, 50u, 107u, 120u, 7u, 7u, 90u, 19u, 31u, 124u, 96u, 88u, 146u, 145u, 117u, 140u, 25u, 147u, 172u, 59u, 30u, 213u, 164u, 187u, 53u, 226u, 48u, 0u, 147u, 246u, 254u, 122u, 194u, 171u, 246u, 248u, 62u, 62u, 176u, 142u, 0u, 230u, 95u, 13u, 226u, 215u, 143u, 237u, 235u, 105u, 59u, 139u, 87u, 73u, 176u, 16u, 240u, 104u, 7u, 142u, 28u, 123u, 226u, 228u, 179u, 7u, 255u, 70u, 61u, 70u, 5u, 220u, 20u, 39u, 249u, 110u, 242u, 38u, 42u, 14u, 74u, 214u, 19u, 232u, 127u, 157u, 113u, 149u, 151u, 185u, 18u, 149u, 23u, 180u, 252u, 62u, 31u, 31u, 249u, 90u, 38u, 77u, 24u, 188u, 38u, 111u, 143u, 31u, 137u, 70u, 73u, 80u, 141u, 145u, 248u, 97u, 158u, 53u, 39u, 156u, 179u, 135u, 158u, 222u, 148u, 117u, 165u, 40u, 254u, 210u, 66u, 138u, 135u, 141u, 159u, 80u, 13u, 169u, 236u, 202u, 223u, 178u, 185u, 136u, 192u, 158u, 237u, 157u, 107u, 162u, 207u, 111u, 228u, 14u, 55u, 48u, 191u, 124u, 190u, 201u, 48u, 194u, 173u, 82u, 99u, 223u, 124u, 103u, 30u, 79u, 139u, 174u, 234u, 185u, 233u, 180u, 91u, 53u, 248u, 196u, 188u, 231u, 77u, 229u, 144u, 9u, 250u, 184u, 115u, 146u, 40u, 238u, 217u, 135u, 179u, 28u, 227u, 31u, 246u, 203u, 221u, 104u, 140u, 32u, 85u, 186u, 59u, 145u, 155u, 32u, 92u, 89u, 195u, 179u, 36u, 13u, 21u, 220u, 75u, 82u, 126u, 59u, 62u, 187u, 62u, 188u, 203u, 5u, 19u, 14u, 107u, 66u, 236u, 128u, 231u, 185u, 180u, 159u, 13u, 70u, 186u, 245u, 174u, 85u, 209u, 220u, 91u, 115u, 76u, 45u, 238u, 121u, 141u, 166u, 205u, 102u, 86u, 186u, 144u, 17u, 63u, 221u, 10u, 39u, 174u, 189u, 182u, 251u, 215u, 222u, 102u, 176u, 207u, 251u, 233u, 18u, 209u, 217u, 226u, 123u, 34u, 231u, 124u, 168u, 235u, 19u, 248u, 43u, 253u, 43u, 58u, 223u, 216u, 229u, 156u, 70u, 241u, 21u, 164u, 151u, 39u, 253u, 26u, 16u, 77u, 128u, 16u, 237u, 36u, 139u, 250u, 192u, 226u, 54u, 50u, 169u, 181u, 18u, 15u, 179u, 133u, 194u, 95u, 248u, 231u, 109u, 113u, 93u, 241u, 188u, 2u, 230u, 83u, 79u, 39u, 146u, 32u, 151u, 150u, 182u, 12u, 7u, 12u, 73u, 151u, 191u, 230u, 170u, 73u, 249u, 52u, 200u, 176u, 66u, 98u, 74u, 3u, 119u, 227u, 239u, 73u, 92u, 80u, 81u, 15u, 99u, 185u, 52u)
        var secret : List<UByte> = listOf(124u, 230u, 18u, 30u, 212u, 117u, 106u, 175u, 141u, 208u, 177u, 22u, 206u, 183u, 244u, 74u, 178u, 241u, 29u, 79u, 76u, 175u, 89u, 36u, 228u, 189u, 7u, 3u, 83u, 115u, 158u, 106u, 60u, 139u, 3u, 156u, 222u, 117u, 237u, 194u, 19u, 76u, 127u, 247u, 107u, 202u, 93u, 122u, 222u, 63u, 229u, 155u, 215u, 145u, 243u, 231u, 62u, 220u, 151u, 225u, 136u, 193u, 228u, 82u, 28u)

        var plaintext_bytes = uniffi.corecrypto.decrypt(
            payload,
            hkdf_salt,
            gcm_nonce,
            secret,
            listOf(),
        )
        var expected = "0a88030ac00108b08b90bfe53012220a20b1d1ae465df4258351c462ea592723753a366263146c69120b4901e4c7a56c8b1a920108b08b90bfe53012440a420a401051d42da81190bbbe080f0cef3356cb476ecf87b112b22a4623f1d22ac358fa08a6160720051acf6ac651335c9114a052a7885ecfaf7c9725f9700075ac22b11a430a41046520443dc4358499e8f0269567bcc27d7264771de694eb84d5c5334e152ede227f3a1606b6dd47129d7c999a6655855cb02dc2b32ee9bf02c01578277dd4ddeb12c20108d88b90bfe53012220a20744cabc19d4d84d9753eed7091bc3047d2e46578cce75193add548f530c7f1d31a940108d88b90bfe53012460a440a409e12294d043420f762ed24e7d21f26328f0f787a964d07f7ebf288f2ab9f750b76b820339ff8cffd4be83adf7177fd29265c4479bf9ab4dc8ed9e5af399a9fab10011a430a4104e0f94416fc0431050a7f4561f8dfdd89e23d24c1d05c50710ef0524316a3bd5ed938c0f111133348fc2aeff399838ce3bd8505182e8582efc6beda0d5144330f"
        var result_hex = plaintext_bytes.joinToString("") {
            "%02x".format(it.toInt())
        }
        println(expected)
        println(result_hex)
        assert(expected.equals(result_hex))

        textView.text = "Decrypted contents match! Expected = $expected and got = $result_hex)"
    }
}
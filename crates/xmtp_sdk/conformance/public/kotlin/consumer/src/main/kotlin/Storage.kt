import android.content.Context
import uniffi.xmtp_sdk.*

// The Android overload resolves the default directory from a real Context.
fun consumeAndroidStorage(context: Context): StorageOptions = StorageOptions(context, label = "consumer")

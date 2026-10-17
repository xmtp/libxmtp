package uniffi.xmtp_sdk

import android.content.Context
import java.io.File

/** Resolve Android default storage from the app's internal files directory. */
fun StorageOptions(
    context: Context,
    label: String? = null,
    encryptionKey: ByteArray? = null,
    pool: StoragePoolOptions? = null,
    singleConnection: Boolean = false,
): StorageOptions =
    StorageOptions(
        location = StorageLocation.Directory(File(context.filesDir, "xmtp_db").absolutePath),
        label = label,
        encryptionKey = encryptionKey,
        pool = pool,
        singleConnection = singleConnection,
    )

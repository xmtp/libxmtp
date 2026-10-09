package org.xmtp.android.example.messenger

internal fun ownedMetadataBackend(
    url: String?,
    admitted: String?,
): String {
    require(admitted == "true") { "Metadata test requires owned fixture admission" }
    val target = requireNotNull(url) { "Missing owned metadata backend" }
    val parsed = java.net.URI(target)
    require(
        parsed.scheme == "http" && parsed.host == "127.0.0.1" && parsed.port in 1..65535 &&
            parsed.rawUserInfo == null && parsed.rawPath.isNullOrEmpty() &&
            parsed.rawQuery == null && parsed.rawFragment == null &&
            target == "http://127.0.0.1:${parsed.port}",
    ) { "Invalid owned metadata backend" }
    return target
}

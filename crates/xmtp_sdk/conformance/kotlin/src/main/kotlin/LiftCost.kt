import uniffi.xmtp_sdk.FfiConverterTypeMessage
import uniffi.xmtp_sdk.FfiConverterTypeMessageData
import uniffi.xmtp_sdk.MessageData

// Deferred performance gate. Do not call until the owner enables benchmarks.
fun lift_cost_within_5_percent(sample: MessageData) {
    val count = 10_000
    val runs = 20
    val plain = mutableListOf<Long>()
    val hosted = mutableListOf<Long>()
    var observed = 0
    repeat(runs) {
        val startPlain = System.nanoTime()
        repeat(count) {
            val record = FfiConverterTypeMessageData.lift(FfiConverterTypeMessageData.lower(sample))
            observed += record.id.toString().length
        }
        plain += System.nanoTime() - startPlain

        val startHost = System.nanoTime()
        repeat(count) {
            val message = FfiConverterTypeMessage.lift(FfiConverterTypeMessageData.lower(sample))
            observed += message.id.toString().length
        }
        hosted += System.nanoTime() - startHost
    }
    check(observed > 0)
    check(hosted.sorted()[runs / 2] <= plain.sorted()[runs / 2] * 1.05)
}

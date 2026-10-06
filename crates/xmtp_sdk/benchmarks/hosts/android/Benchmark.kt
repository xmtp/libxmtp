package org.xmtp.benchmark

import android.app.Instrumentation
import android.os.Bundle
import android.os.Debug
import android.os.SystemClock
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.takeWhile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

fun now(): Double = SystemClock.elapsedRealtimeNanos() / 1_000_000.0

fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }

fun unhex(value: String): ByteArray = value.chunked(2).map { it.toInt(16).toByte() }.toByteArray()

fun obj(vararg pairs: Pair<String, Any?>): JSONObject =
    JSONObject().apply {
        pairs.forEach { (key, value) -> put(key, value ?: JSONObject.NULL) }
    }

fun JSONArray.objects(): List<JSONObject> = (0 until length()).map(::getJSONObject)

class HostConfig(
    val context: android.content.Context,
    val data: JSONObject,
) {
    val backend: String get() = data.getString("backend_url")

    fun signer(input: JSONObject = JSONObject()): JSONObject {
        val connection = URL(data.getString("signer_url")).openConnection() as HttpURLConnection
        connection.requestMethod = "POST"
        connection.doOutput = true
        connection.setRequestProperty("Content-Type", "application/json")
        connection.outputStream.use { it.write(input.toString().toByteArray()) }
        check(connection.responseCode == 200) { "Signer helper failed" }
        return connection.inputStream
            .bufferedReader()
            .use {
                JSONObject(
                    it.readText(),
                )
            }.also { connection.disconnect() }
    }
}

suspend fun seed(
    config: HostConfig,
    fixture: JSONObject,
    root: File,
    prefix: String,
    streaming: Boolean,
): JSONObject {
    val a = config.signer()
    val b = config.signer()
    val state =
        obj(
            "senderKey" to a.getString("key"),
            "senderAddress" to a.getString("address"),
            "senderPath" to File(root, "$prefix-sender").path,
            "receiverKey" to b.getString("key"),
            "receiverAddress" to b.getString("address"),
            "receiverPath" to File(root, "$prefix-receiver").path,
        )
    val sender =
        benchCreate(
            config,
            state.getString("senderKey"),
            state.getString("senderAddress"),
            state.getString("senderPath"),
        )
    var receiver: BenchClient? = null
    try {
        state.put("senderInbox", benchInbox(sender))
        if (streaming) {
            receiver =
                benchCreate(
                    config,
                    state.getString("receiverKey"),
                    state.getString("receiverAddress"),
                    state.getString("receiverPath"),
                )
            state.put("receiverInbox", benchInbox(receiver))
        }
        val group = benchNewGroup(sender, if (streaming) listOf(state.getString("receiverInbox")) else emptyList())
        state.put("groupId", benchGroupID(group))
        if (receiver != null) {
            benchSync(receiver)
            benchGroup(receiver, state.getString("groupId"))
        }
        val ids = mutableListOf<String>()
        val events = mutableListOf<String>()
        for (row in fixture.getJSONArray("messages").objects()) {
            val id = benchPrepare(group, row, ids, benchInbox(sender))
            ids += id
            events += id
            for (reaction in row.getJSONArray("reactions").objects()) {
                events += benchReact(group, id, benchInbox(sender), reaction)
            }
        }
        state.put("ids", JSONArray(ids))
        state.put("eventIds", JSONArray(events))
        if (!streaming) {
            benchPublish(group)
            benchGroupSync(group)
        }
        return state
    } finally {
        if (receiver != null) benchClose(receiver)
        benchClose(sender)
    }
}

class Benchmark : Instrumentation() {
    override fun onCreate(arguments: Bundle) {
        super.onCreate(arguments)
        Thread {
            val result = Bundle()
            try {
                val input = File(targetContext.getExternalFilesDir(null), "benchmark-input")
                val request = JSONObject(File(input, "request.json").readText())
                val fixture = JSONObject(File(input, "fixture.json").readText())
                val config = HostConfig(targetContext, JSONObject(File(input, "host.json").readText()))
                val root = File(targetContext.filesDir, request.getString("state_key")).apply { mkdirs() }
                val active = AtomicBoolean(true)
                val peak = AtomicLong(0)
                val sampler =
                    Thread {
                        while (active.get()) {
                            val info = Debug.MemoryInfo()
                            Debug.getMemoryInfo(info)
                            peak.updateAndGet { maxOf(it, info.totalPss.toLong() * 1024) }
                            Thread.sleep(10)
                        }
                    }.apply { start() }
                val response: JSONObject
                try {
                    response = runBlocking { perform(config, fixture, root, request) }
                } finally {
                    active.set(false)
                    sampler.join()
                }
                response.put("peak_memory_bytes", peak.get())
                File(input, "response.json").writeText(response.toString())
                result.putString("benchmark", "complete")
                finish(0, result)
            } catch (error: Throwable) {
                result.putString("benchmark_error", error.stackTraceToString())
                finish(1, result)
            }
        }.start()
    }
}

suspend fun perform(
    config: HostConfig,
    fixture: JSONObject,
    root: File,
    request: JSONObject,
): JSONObject {
    val phase = request.getString("phase")
    val workload = request.optString("workload")
    val sample = request.optInt("sample")
    if (phase == "setup") {
        File(root, "page.json").writeText(seed(config, fixture, root, "page", false).toString())
        return obj("ready" to true)
    }
    if (phase == "cleanup") {
        // The launcher deletes a run's client databases with this request.
        check(root.deleteRecursively()) { "Could not delete $root" }
        return obj("ready" to true)
    }
    if (phase == "reset") {
        if (workload == "stream") {
            File(root, "stream-$sample.json").writeText(seed(config, fixture, root, "stream-$sample", true).toString())
        }
        return obj("ready" to true)
    }
    check(phase == "measure") { "Unknown request phase $phase" }
    if (workload == "cold_start") {
        val account = config.signer()
        val start = now()
        val client =
            benchCreate(config, account.getString("key"), account.getString("address"), File(root, "cold-$sample").path)
        val finish = now()
        benchClose(client)
        return obj("duration_ms" to finish - start)
    }
    val state = JSONObject(File(root, if (workload == "stream") "stream-$sample.json" else "page.json").readText())
    val sender =
        benchOpen(
            config,
            state.getString("senderAddress"),
            state.getString("senderPath"),
            state.getString("senderInbox"),
        )
    try {
        val group = benchGroup(sender, state.getString("groupId"))
        val ids = state.getJSONArray("ids")
        val keys =
            (0 until ids.length()).associate {
                ids.getString(it) to
                    it.toString()
            }
        if (workload == "page") {
            val start = now()
            val page = benchPage(group, 1000, keys)
            return obj("duration_ms" to now() - start, "observed_messages" to JSONArray(page))
        }
        val receiver =
            benchOpen(
                config,
                state.getString("receiverAddress"),
                state.getString("receiverPath"),
                state.getString("receiverInbox"),
            )
        try {
            val receivedGroup = benchGroup(receiver, state.getString("groupId"))
            val expectedArray = state.getJSONArray("eventIds")
            val expected = (0 until expectedArray.length()).map { expectedArray.getString(it) }.toSet()
            val seen = mutableSetOf<String>()
            // Set at the last expected event. When takeWhile stops, the flow
            // ends its reader before collect returns, so the timer stops here.
            var lastEvent = 0.0
            return coroutineScope {
                // A failure in the reader or the publisher cancels the other
                // one, and coroutineScope waits for both before the closes.
                val collecting =
                    async {
                        benchStream(receiver, receivedGroup)
                            .takeWhile { message ->
                                if (message.id in expected) {
                                    check(seen.add(message.id)) { "Duplicate expected stream event" }
                                }
                                if (seen.size == expected.size) lastEvent = now()
                                seen.size != expected.size
                            }.collect { }
                    }
                // The reader opens the subscription during an untimed grace period.
                delay(1000)
                val start = now()
                benchPublish(group)
                val published = now()
                // This also waits for the reader to end, outside the timer.
                collecting.await()
                check(seen == expected) { "Stream ended with missing fixture messages" }
                obj("duration_ms" to maxOf(published, lastEvent) - start, "streamed_events" to seen.size)
            }
        } finally {
            benchClose(receiver)
        }
    } finally {
        benchClose(sender)
    }
}

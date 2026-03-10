package com.amadeuz.notes

import android.os.Handler
import android.os.Looper
import com.google.gson.Gson
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.TimeUnit

class SyncService(
    private val url: String,
    private val onMessage: (WSMsg) -> Unit,
    private val onConnectionChange: (Boolean) -> Unit
) {
    private val client = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .build()
    private val gson = Gson()
    private val handler = Handler(Looper.getMainLooper())
    private var socket: WebSocket? = null
    private var alive = true

    init { connect() }

    private fun connect() {
        if (!alive) return
        val request = Request.Builder().url(url).build()
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, text: String) {
                try {
                    val msg = gson.fromJson(text, WSMsg::class.java) ?: return
                    onConnectionChange(true)
                    onMessage(msg)
                } catch (_: Exception) {}
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                onConnectionChange(false)
                reconnectDelayed()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                onConnectionChange(false)
                reconnectDelayed()
            }
        })
    }

    private fun reconnectDelayed() {
        if (!alive) return
        handler.postDelayed({ connect() }, 3_000)
    }

    fun send(msg: WSMsg) {
        socket?.send(gson.toJson(msg))
    }

    fun close() {
        alive = false
        handler.removeCallbacksAndMessages(null)
        socket?.close(1000, null)
    }
}

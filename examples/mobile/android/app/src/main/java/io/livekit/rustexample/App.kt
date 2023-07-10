package io.livekit.rustexample

class App {
    init {
        System.loadLibrary("mobile")
    }

    external fun connect(url: String, token: String)
}
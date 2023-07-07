package io.livekit.rustexample

import android.app.Application

class App : Application() {
    init {
        System.loadLibrary("mobile")
    }

    external fun connect(url: String, token: String)

    override fun onCreate() {
        super.onCreate()
    }
}
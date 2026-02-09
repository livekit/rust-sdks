package io.livekit.rustexample

import android.util.Log

class App {
    private val nativeAvailable: Boolean

    init {
        nativeAvailable = try {
            System.loadLibrary("mobile")
            true
        } catch (error: UnsatisfiedLinkError) {
            Log.w(TAG, "Native library not found; skipping LiveKit init.", error)
            false
        }
    }

    // Connection methods
    private external fun connectNative(url: String, token: String)
    private external fun disconnectNative()
    private external fun isConnectedNative(): Boolean

    // Audio methods
    private external fun pushAudioNative(samples: ShortArray): Int
    private external fun pullAudioNative(buffer: ShortArray): Int
    private external fun getPlaybackBufferSizeNative(): Int

    fun connect(url: String, token: String) {
        if (!nativeAvailable) {
            Log.w(TAG, "LiveKit native library unavailable; connect() ignored.")
            return
        }
        connectNative(url, token)
    }

    fun disconnect() {
        if (!nativeAvailable) {
            Log.w(TAG, "LiveKit native library unavailable; disconnect() ignored.")
            return
        }
        disconnectNative()
    }

    fun isConnected(): Boolean {
        if (!nativeAvailable) {
            return false
        }
        return isConnectedNative()
    }

    /**
     * Push captured microphone audio to LiveKit.
     * @param samples 16-bit PCM samples (mono, 48kHz expected)
     * @return Number of samples consumed
     */
    fun pushAudio(samples: ShortArray): Int {
        if (!nativeAvailable) {
            return 0
        }
        return pushAudioNative(samples)
    }

    /**
     * Pull playback audio from LiveKit (remote participants).
     * @param buffer Buffer to fill with 16-bit PCM samples
     * @return Number of actual samples written (rest is silence)
     */
    fun pullAudio(buffer: ShortArray): Int {
        if (!nativeAvailable) {
            return 0
        }
        return pullAudioNative(buffer)
    }

    /**
     * Get the number of samples available in the playback buffer.
     * Useful for monitoring buffer health.
     */
    fun getPlaybackBufferSize(): Int {
        if (!nativeAvailable) {
            return 0
        }
        return getPlaybackBufferSizeNative()
    }

    fun isNativeAvailable(): Boolean = nativeAvailable

    private companion object {
        private const val TAG = "LiveKitApp"
    }
}

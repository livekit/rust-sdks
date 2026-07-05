package io.livekit.rustexample

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.util.Log
import androidx.core.content.ContextCompat
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean

class MediaManager(
    private val context: Context,
    private val app: App  // Reference to App for pushing/pulling audio
) {

    companion object {
        private const val TAG = "MediaManager"

        // Audio configuration - must match Rust side
        const val SAMPLE_RATE = 48000
        const val CHANNEL_CONFIG_IN = AudioFormat.CHANNEL_IN_MONO
        const val CHANNEL_CONFIG_OUT = AudioFormat.CHANNEL_OUT_MONO
        const val AUDIO_FORMAT = AudioFormat.ENCODING_PCM_16BIT
        const val FRAME_DURATION_MS = 10
        const val SAMPLES_PER_FRAME = SAMPLE_RATE * FRAME_DURATION_MS / 1000  // 480 samples
        const val BYTES_PER_SAMPLE = 2 // 16-bit PCM
        const val BUFFER_SIZE_FRAMES = 10
    }

    private var audioRecord: AudioRecord? = null
    private var audioTrack: AudioTrack? = null

    private var captureThread: Thread? = null
    private var playbackThread: Thread? = null

    private val isCapturing = AtomicBoolean(false)
    private val isPlaying = AtomicBoolean(false)

    fun hasRecordPermission(): Boolean {
        return ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.RECORD_AUDIO
        ) == PackageManager.PERMISSION_GRANTED
    }

    fun startMicrophone(): Boolean {
        if (!hasRecordPermission()) {
            Log.e(TAG, "RECORD_AUDIO permission not granted")
            return false
        }

        if (isCapturing.get()) {
            Log.w(TAG, "Microphone already started")
            return true
        }

        val minBufferSize = AudioRecord.getMinBufferSize(
            SAMPLE_RATE,
            CHANNEL_CONFIG_IN,
            AUDIO_FORMAT
        )

        if (minBufferSize == AudioRecord.ERROR || minBufferSize == AudioRecord.ERROR_BAD_VALUE) {
            Log.e(TAG, "Failed to get minimum buffer size for AudioRecord")
            return false
        }

        val bufferSize = maxOf(minBufferSize, SAMPLES_PER_FRAME * BYTES_PER_SAMPLE * BUFFER_SIZE_FRAMES)

        try {
            audioRecord = AudioRecord(
                MediaRecorder.AudioSource.VOICE_COMMUNICATION,
                SAMPLE_RATE,
                CHANNEL_CONFIG_IN,
                AUDIO_FORMAT,
                bufferSize
            )

            if (audioRecord?.state != AudioRecord.STATE_INITIALIZED) {
                Log.e(TAG, "AudioRecord failed to initialize")
                audioRecord?.release()
                audioRecord = null
                return false
            }

            audioRecord?.startRecording()
            isCapturing.set(true)

            captureThread = Thread({
                android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_URGENT_AUDIO)
                captureLoop()
            }, "AudioCaptureThread")
            captureThread?.start()

            Log.i(TAG, "Microphone started successfully")
            return true

        } catch (e: SecurityException) {
            Log.e(TAG, "SecurityException starting microphone", e)
            return false
        } catch (e: Exception) {
            Log.e(TAG, "Exception starting microphone", e)
            audioRecord?.release()
            audioRecord = null
            return false
        }
    }

    fun stopMicrophone() {
        if (!isCapturing.get()) {
            return
        }

        isCapturing.set(false)

        captureThread?.let { thread ->
            try {
                thread.join(1000)
            } catch (e: InterruptedException) {
                Log.w(TAG, "Interrupted while waiting for capture thread")
            }
        }
        captureThread = null

        audioRecord?.let { record ->
            try {
                record.stop()
            } catch (e: Exception) {
                Log.w(TAG, "Exception stopping AudioRecord", e)
            }
            record.release()
        }
        audioRecord = null

        Log.i(TAG, "Microphone stopped")
    }

    fun startSpeaker(): Boolean {
        if (isPlaying.get()) {
            Log.w(TAG, "Speaker already started")
            return true
        }

        val minBufferSize = AudioTrack.getMinBufferSize(
            SAMPLE_RATE,
            CHANNEL_CONFIG_OUT,
            AUDIO_FORMAT
        )

        if (minBufferSize == AudioTrack.ERROR || minBufferSize == AudioTrack.ERROR_BAD_VALUE) {
            Log.e(TAG, "Failed to get minimum buffer size for AudioTrack")
            return false
        }

        val bufferSize = maxOf(minBufferSize, SAMPLES_PER_FRAME * BYTES_PER_SAMPLE * BUFFER_SIZE_FRAMES)

        try {
            val audioAttributes = AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                .build()

            val audioFormat = AudioFormat.Builder()
                .setSampleRate(SAMPLE_RATE)
                .setChannelMask(CHANNEL_CONFIG_OUT)
                .setEncoding(AUDIO_FORMAT)
                .build()

            audioTrack = AudioTrack(
                audioAttributes,
                audioFormat,
                bufferSize,
                AudioTrack.MODE_STREAM,
                android.media.AudioManager.AUDIO_SESSION_ID_GENERATE
            )

            if (audioTrack?.state != AudioTrack.STATE_INITIALIZED) {
                Log.e(TAG, "AudioTrack failed to initialize")
                audioTrack?.release()
                audioTrack = null
                return false
            }

            audioTrack?.play()
            isPlaying.set(true)

            playbackThread = Thread({
                android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_URGENT_AUDIO)
                playbackLoop()
            }, "AudioPlaybackThread")
            playbackThread?.start()

            Log.i(TAG, "Speaker started successfully")
            return true

        } catch (e: Exception) {
            Log.e(TAG, "Exception starting speaker", e)
            audioTrack?.release()
            audioTrack = null
            return false
        }
    }

    fun stopSpeaker() {
        if (!isPlaying.get()) {
            return
        }

        isPlaying.set(false)

        playbackThread?.let { thread ->
            try {
                thread.join(1000)
            } catch (e: InterruptedException) {
                Log.w(TAG, "Interrupted while waiting for playback thread")
            }
        }
        playbackThread = null

        audioTrack?.let { track ->
            try {
                track.stop()
            } catch (e: Exception) {
                Log.w(TAG, "Exception stopping AudioTrack", e)
            }
            track.release()
        }
        audioTrack = null

        Log.i(TAG, "Speaker stopped")
    }

    fun startAll(): Boolean {
        val micStarted = startMicrophone()
        val speakerStarted = startSpeaker()
        return micStarted && speakerStarted
    }

    fun stopAll() {
        stopMicrophone()
        stopSpeaker()
    }

    fun isCapturing(): Boolean = isCapturing.get()
    fun isPlaying(): Boolean = isPlaying.get()

    private fun captureLoop() {
        val frameSize = SAMPLES_PER_FRAME * BYTES_PER_SAMPLE
        val byteBuffer = ByteArray(frameSize)
        val shortBuffer = ShortArray(SAMPLES_PER_FRAME)

        Log.d(TAG, "Capture loop started, frame size: $frameSize bytes, $SAMPLES_PER_FRAME samples")

        var frameCount = 0L

        while (isCapturing.get()) {
            val record = audioRecord ?: break

            // Read bytes from AudioRecord
            val bytesRead = record.read(byteBuffer, 0, frameSize)

            when {
                bytesRead > 0 -> {
                    // Convert bytes to shorts
                    val samplesRead = bytesRead / BYTES_PER_SAMPLE
                    ByteBuffer.wrap(byteBuffer, 0, bytesRead)
                        .order(ByteOrder.LITTLE_ENDIAN)
                        .asShortBuffer()
                        .get(shortBuffer, 0, samplesRead)

                    // Push to LiveKit via native code
                    val consumed = app.pushAudio(shortBuffer.copyOf(samplesRead))

                    frameCount++
                    if (frameCount % 100 == 0L) {  // Log every ~1 second
                        Log.d(TAG, "Captured and pushed $samplesRead samples (frame $frameCount)")
                    }
                }
                bytesRead == AudioRecord.ERROR_INVALID_OPERATION -> {
                    Log.e(TAG, "AudioRecord ERROR_INVALID_OPERATION")
                    break
                }
                bytesRead == AudioRecord.ERROR_BAD_VALUE -> {
                    Log.e(TAG, "AudioRecord ERROR_BAD_VALUE")
                    break
                }
                bytesRead == AudioRecord.ERROR_DEAD_OBJECT -> {
                    Log.e(TAG, "AudioRecord ERROR_DEAD_OBJECT")
                    break
                }
                bytesRead == AudioRecord.ERROR -> {
                    Log.e(TAG, "AudioRecord ERROR")
                    break
                }
            }
        }

        Log.d(TAG, "Capture loop ended after $frameCount frames")
    }

    private fun playbackLoop() {
        val shortBuffer = ShortArray(SAMPLES_PER_FRAME)
        val byteBuffer = ByteArray(SAMPLES_PER_FRAME * BYTES_PER_SAMPLE)

        Log.d(TAG, "Playback loop started, frame size: $SAMPLES_PER_FRAME samples")

        var frameCount = 0L
        var silentFrames = 0L

        while (isPlaying.get()) {
            val track = audioTrack ?: break

            // Pull audio from LiveKit via native code
            val samplesReceived = app.pullAudio(shortBuffer)

            // Convert shorts to bytes for AudioTrack
            ByteBuffer.wrap(byteBuffer)
                .order(ByteOrder.LITTLE_ENDIAN)
                .asShortBuffer()
                .put(shortBuffer)

            val bytesWritten = track.write(byteBuffer, 0, SAMPLES_PER_FRAME * BYTES_PER_SAMPLE)

            frameCount++
            if (samplesReceived == 0) {
                silentFrames++
            }

            if (frameCount % 100 == 0L) {  // Log every ~1 second
                val bufferSize = app.getPlaybackBufferSize()
                Log.d(TAG, "Playback frame $frameCount: received $samplesReceived samples, " +
                        "buffer size: $bufferSize, silent frames: $silentFrames")
                silentFrames = 0
            }

            when {
                bytesWritten < 0 -> {
                    when (bytesWritten) {
                        AudioTrack.ERROR_INVALID_OPERATION -> {
                            Log.e(TAG, "AudioTrack ERROR_INVALID_OPERATION")
                        }
                        AudioTrack.ERROR_BAD_VALUE -> {
                            Log.e(TAG, "AudioTrack ERROR_BAD_VALUE")
                        }
                        AudioTrack.ERROR_DEAD_OBJECT -> {
                            Log.e(TAG, "AudioTrack ERROR_DEAD_OBJECT")
                            break
                        }
                        else -> {
                            Log.e(TAG, "AudioTrack error: $bytesWritten")
                        }
                    }
                }
            }
        }

        Log.d(TAG, "Playback loop ended after $frameCount frames")
    }

    fun release() {
        stopAll()
    }
}
